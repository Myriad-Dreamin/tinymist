import { PreviewMode } from "typst-dom/typst-doc.mjs";
import { TypstPreviewDocument as TypstDocument } from "typst-dom/index.preview.mjs";
import {
    rendererBuildInfo,
    createTypstRenderer,
} from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import renderModule from "@myriaddreamin/typst-ts-renderer/pkg/typst_ts_renderer_bg.wasm?url";
// @ts-ignore
// import { RenderSession as RenderSession2 } from "@myriaddreamin/typst-ts-renderer/pkg/wasm-pack-shim.mjs";
import { RenderSession } from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import { WebSocketSubject, webSocket } from 'rxjs/webSocket';
import { Subject, Subscription, buffer, debounceTime, fromEvent, tap } from "rxjs";
export { PreviewMode } from 'typst-dom/typst-doc.mjs';

// for debug propose
// queryObjects((window as any).TypstRenderSession);
(window as any).TypstRenderSession = RenderSession;
// (window as any).TypstRenderSessionKernel = RenderSession2;

const enc = new TextEncoder();
const dec = new TextDecoder();
const NOT_AVAILABLE = "current not available";
const COMMA = enc.encode(",");
export interface WsArgs {
    url: string;
    previewMode: PreviewMode;
    isContentPreview: boolean;
}

export async function wsMain({ url, previewMode, isContentPreview }: WsArgs) {
    if (!url) {
        const hookedElem = document.getElementById("typst-app");
        if (hookedElem) {
            hookedElem.innerHTML = "";
        }
        return () => { };
    }

    let disposed = false;
    let $ws: WebSocketSubject<ArrayBuffer> | undefined = undefined;
    const subsribes: Subscription[] = [];

    function createSvgDocument(kModule: RenderSession) {
        const hookedElem = document.getElementById("typst-app")!;
        if (hookedElem.firstElementChild?.tagName !== "svg") {
            hookedElem.innerHTML = "";
        }
        const resizeTarget = document.getElementById('typst-container-main')!;

        const svgDoc = new TypstDocument({
            hookedElem,
            kModule,
            previewMode,
            isContentPreview,
            // set rescale target to `body`
            retrieveDOMState() {
                return {
                    // reserving 1px to hide width border
                    width: resizeTarget.clientWidth + 1,
                    // reserving 1px to hide width border
                    height: resizeTarget.offsetHeight,
                    boundingRect: resizeTarget.getBoundingClientRect(),
                };
            },
        });

        // drag (panal resizing) -> rescaling
        // window.onresize = () => svgDoc.rescale();
        subsribes.push(
            fromEvent(window, "resize").
                subscribe(() => svgDoc.addViewportChange())
        );

        if (!isContentPreview) {
            subsribes.push(
                fromEvent(window, "scroll").
                    pipe(debounceTime(500)).
                    subscribe(() => svgDoc.addViewportChange())
            );
        }

        // Handle messages sent from the extension to the webview
        subsribes.push(
            fromEvent<MessageEvent>(window, "message").
                subscribe(event => {
                    const message = event.data; // The json data that the extension sent
                    switch (message.type) {
                        case 'outline': {
                            svgDoc.setOutineData(message.outline);
                            break;
                        }
                    }
                })
        );

        if (previewMode === PreviewMode.Slide) {
            {
                const inpPageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;
                if (inpPageSelector) {
                    inpPageSelector.addEventListener("input", () => {
                        if (inpPageSelector.value.length === 0) {
                            return;
                        }
                        const page = Number.parseInt(inpPageSelector.value);
                        svgDoc.setPartialPageNumber(page);
                    });
                }
            }

            const focusInput = () => {
                const inpPageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;
                if (inpPageSelector) {
                    inpPageSelector.focus();
                }
            }

            const blurInput = () => {
                const inpPageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;
                if (inpPageSelector) {
                    inpPageSelector.blur();
                }
            }

            const updateDiff = (diff: number) => () => {
                const pageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;

                if (pageSelector) {
                    console.log("updateDiff", diff);
                    const v = pageSelector.value;
                    if (v.length === 0) {
                        return;
                    }
                    const page = Number.parseInt(v) + diff;
                    if (page <= 0) {
                        return;
                    }
                    if (svgDoc.setPartialPageNumber(page)) {
                        pageSelector.value = page.toString();
                        blurInput();
                    }
                }
            }

            const updatePrev = updateDiff(-1);
            const updateNext = updateDiff(1);

            const pagePrevSelector = document.getElementById("typst-page-prev-selector");
            if (pagePrevSelector) {
                pagePrevSelector.addEventListener("click", updatePrev);
            }
            const pageNextSelector = document.getElementById("typst-page-next-selector");
            if (pageNextSelector) {
                pageNextSelector.addEventListener("click", updateNext);
            }

            const toggleHelp = () => {
                const help = document.getElementById("typst-help-panel");
                console.log("toggleHelp", help);
                if (help) {
                    help.classList.toggle("hidden");
                }
            };

            const removeHelp = () => {
                const help = document.getElementById("typst-help-panel");
                if (help) {
                    help.classList.add("hidden");
                }
            }

            const helpButton = document.getElementById("typst-top-help-button");
            helpButton?.addEventListener('click', toggleHelp);

            window.addEventListener("keydown", (e) => {
                let handled = true;
                switch (e.key) {
                    case "ArrowLeft":
                    case "ArrowUp":
                        blurInput();
                        removeHelp();
                        updatePrev();
                        break;
                    case " ":
                    case "ArrowRight":
                    case "ArrowDown":
                        blurInput();
                        removeHelp();
                        updateNext();
                        break;
                    case "h":
                        blurInput();
                        toggleHelp();
                        break;
                    case "g":
                        removeHelp();
                        focusInput();
                        break;
                    case "Escape":
                        removeHelp();
                        blurInput();
                        handled = false;
                        break;
                    default:
                        handled = false;
                }

                if (handled) {
                    e.preventDefault();
                }
            });
        }

        return svgDoc;
    }

    function setupSocket(svgDoc: TypstDocument): () => void {
        // todo: reconnect setTimeout(() => setupSocket(svgDoc), 1000);
        $ws = webSocket<ArrayBuffer>({
            url,
            binaryType: "arraybuffer",
            serializer: t => t,
            deserializer: (event) => event.data,
            openObserver: {
                next: (e) => {
                    const sock = e.target;
                    console.log('WebSocket connection opened', sock);
                    window.typstWebsocket = sock as any;
                    svgDoc.reset();
                    window.typstWebsocket.send("current");
                }
            },
            closeObserver: {
                next: (e) => {
                    console.log('WebSocket connection closed', e);
                    $ws?.unsubscribe();
                    if (!disposed) {
                        setTimeout(() => setupSocket(svgDoc), 1000);
                    }
                }
            }
        });

        const batchMessageChannel = new Subject<ArrayBuffer>();

        const dispose = () => {
            disposed = true;
            svgDoc.dispose();
            for (const sub of subsribes.splice(0, subsribes.length)) {
                sub.unsubscribe();
            }
            $ws?.complete();
        };

        // window.typstWebsocket = new WebSocket("ws://127.0.0.1:23625");


        $ws.subscribe({
            next: (data) => batchMessageChannel.next(data), // Called whenever there is a message from the server.
            error: err => console.log("WebSocket Error: ", err), // Called if at any point WebSocket API signals some kind of error.
            complete: () => console.log('complete') // Called when connection is closed (for whatever reason).
        });

        subsribes.push(
            batchMessageChannel
                .pipe(buffer(batchMessageChannel.pipe(debounceTime(0))))
                .pipe(tap(dataList => { console.log(`batch ${dataList.length} messages`) }))
                .subscribe((dataList) => {
                    dataList.map(processMessage)
                })
        );

        function processMessage(data: ArrayBuffer) {
            if (!(data instanceof ArrayBuffer)) {
                if (data === NOT_AVAILABLE) {
                    return;
                }

                console.error("WebSocket data is not a ArrayBuffer", data);
                return;
            }

            const buffer = data;
            const messageData = new Uint8Array(buffer);

            const message_idx = messageData.indexOf(COMMA[0]);
            const message = [
                dec.decode(messageData.slice(0, message_idx).buffer),
                messageData.slice(message_idx + 1),
            ];
            console.log('recv', message[0], messageData.length);
            // console.log(message[0], message[1].length);
            if (isContentPreview) {
                // whether to scroll to the content preview when user updates document
                const autoScrollContentPreview = true;
                if (!autoScrollContentPreview && message[0] === "jump") {
                    return;
                }

                // "viewport": viewport change to document doesn't affect content preview
                // "partial-rendering": content previe always render partially
                // "cursor": currently not supported
                if ((message[0] === "viewport" || message[0] === "partial-rendering" || message[0] === "cursor")) {
                    return;
                }
            }

            if (message[0] === "jump" || message[0] === "viewport") {
                // todo: aware height padding
                const current_page_number = svgDoc.getPartialPageNumber();

                let positions = dec
                    .decode((message[1] as any).buffer)
                    .split(",")

                // choose the page, x, y closest to the current page
                const [page, x, y] = positions.reduce((acc, cur) => {
                    const [page, x, y] = cur.split(" ").map(Number);
                    const current_page = current_page_number;
                    if (Math.abs(page - current_page) < Math.abs(acc[0] - current_page)) {
                        return [page, x, y];
                    }
                    return acc;
                }, [Number.MAX_SAFE_INTEGER, 0, 0]);

                let pageToJump = page;

                if (previewMode === PreviewMode.Slide) {
                    const pageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;
                    if (svgDoc.setPartialPageNumber(page)) {
                        if (pageSelector) {
                            pageSelector.value = page.toString();
                        }
                        // pageToJump = 1;
                        // todo: hint location
                        return;
                    } else {
                        return;
                    }
                }

                const rootElem =
                    document.getElementById("typst-app")?.firstElementChild;
                if (rootElem) {
                    /// Note: when it is really scrolled, it will trigger `svgDoc.addViewportChange`
                    /// via `window.onscroll` event
                    window.handleTypstLocation(rootElem, pageToJump, x, y);
                }
                return;
            } else if (message[0] === "cursor") {
                // todo: aware height padding
                const [page, x, y] = dec
                    .decode((message[1] as any).buffer)
                    .split(" ")
                    .map(Number);
                console.log("cursor", page, x, y);
                svgDoc.setCursor(page, x, y);
                svgDoc.addViewportChange(); // todo: synthesizing cursor event
                return;
            } else if (message[0] === "cursor-paths") {
                // todo: aware height padding
                const paths = JSON.parse(dec
                    .decode((message[1] as any).buffer));
                console.log("cursor-paths", paths);
                svgDoc.impl.setCursorPaths(paths);
                return;
            } else if (message[0] === "partial-rendering") {
                console.log("Experimental feature: partial rendering enabled");
                svgDoc.setPartialRendering(true);
                return;
            } else if (message[0] === "invert-colors") {
                const rawStrategy = dec.decode((message[1] as any).buffer).trim();
                const strategy = INVERT_COLORS_STRATEGY.find(t => t === rawStrategy) || (JSON.parse(rawStrategy) as StrategyMap);
                console.log("Experimental feature: invert colors strategy taken:", strategy);
                ensureInvertColors(document.getElementById("typst-app"), strategy);
                return;
            } else if (message[0] === "outline") {
                console.log("Experimental feature: outline rendering");
                return;
            }

            svgDoc.addChangement(message as any);
        };

        return dispose;
    }

    let plugin = createTypstRenderer();
    await plugin.init({ getModule: () => renderModule });

    return new Promise<() => void>((resolveDispose) =>
        plugin.runWithSession(kModule /* module kernel from wasm */ => {
            return new Promise(async (kernelDispose) => {
                console.log("plugin initialized, build info:", await rendererBuildInfo());

                const wsDispose = setupSocket(createSvgDocument(kModule));

                // todo: plugin init and setup socket at the same time
                resolveDispose(() => {
                    // dispose ws first
                    wsDispose();
                    // dispose kernel then
                    kernelDispose(undefined);
                });
            })
        }));
};

/** The strategy to set invert colors, see editors/vscode/package.json for enum descriptions */
const INVERT_COLORS_STRATEGY = ['never', 'auto', 'always'] as const;
/** The value of strategy constant */
type StrategyKey = (typeof INVERT_COLORS_STRATEGY)[number];
/** The map from element kinds to strategy */
type StrategyMap = Partial<Record<'rest' | 'image', StrategyKey>>;

function ensureInvertColors(root: HTMLElement | null, strategy: StrategyKey | StrategyMap) {
    if (!root) {
        return;
    }

    // Uniforms type of strategy to `TargetMap`
    if (typeof strategy === 'string') {
        strategy = { rest: strategy };
    }

    let autoDecision: { value: boolean } | undefined = undefined;
    /** 
     * Handles invert colors mode based on a string enumerated strategy.  
     * @param strategy - The strategy set by user.
     * @returns needInvertColor - Use or not use invert color.
     */
    const decide = (strategy: StrategyKey) => {
        switch (strategy) {
            case 'never': return false;
            default: console.warn("Unknown invert-colors strategy:", strategy); return false;
            case 'auto': return (autoDecision ||= { value: determineInvertColor() }).value;
            case 'always': return true;
        }
    }

    root.classList.toggle("invert-colors", decide(strategy?.rest || 'never'));
    root.classList.toggle("normal-image", !decide(strategy?.image || strategy?.rest || 'never'));

    function determineInvertColor() {
        const vscodeAPI = typeof acquireVsCodeApi !== "undefined";

        if (vscodeAPI) {
            // vscode-dark, high-contrast, vscode-light
            const cls = document.body.classList;
            const themeIsDark = (cls.contains("vscode-dark") || cls.contains("vscode-high-contrast")) &&
                !cls.contains("vscode-light");


            if (themeIsDark) {
                console.log("invert-colors because detected by vscode theme", document.body.className);
                return true;
            }
        } else {
            // prefer dark mode
            if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
                console.log("invert-colors because detected by (prefers-color-scheme: dark)");
                return true;
            }
        }

        console.log("doesn't invert-colors because none of dark mode detected");
        return false;
    }
}

