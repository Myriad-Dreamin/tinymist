import { webSocket, WebSocketSubject } from 'rxjs/webSocket';

async function digestHex(message: string) {
    const hashBuffer = await crypto.subtle.digest('SHA-512', new TextEncoder().encode(message));
    return Array.from(
        new Uint8Array(hashBuffer),
        x => x.toString(16).padStart(2, '0')
    ).join('');
}


function generateCryptoRandom(length: number)
{
    return Array.from(
        window.crypto.getRandomValues(new Uint8Array(length)),
        x => x.toString(16).padStart(2, '0')
    ).join('');
}

interface WebSocketAndSubject {
    websocketSubject: WebSocketSubject<ArrayBuffer>;
    websocket: WebSocket | undefined;
}

function getUnsafeSocketCompat(url: string): Promise<WebSocketAndSubject>
{
    return new Promise((wsresolve) => {
        let prews = webSocket<ArrayBuffer>({
            url,
            binaryType: "arraybuffer",
            serializer: t => t,
            deserializer: (event) => event.data,
            openObserver: {
                next: (e) => {
                    console.log('WebSocket connection opened', e.target);
                    wsresolve({ websocketSubject: prews, websocket: e.target as any});
                }
            },
        });

        prews.subscribe({
            error: () => {},
        }); 

        console.log("Authentication skipped (for compat with typst-preview, triggered by special value of `secret`)");
    });
}

export function getAuthenticatedSocket(url: string, secret: string, dec: TextDecoder, enc: TextEncoder): Promise<WebSocketAndSubject> {
    // Typst-preview doesn't support authentication. For now, we then skip authentication.
    // FIXME: Remove this once we no longer support compatibility with external typst-preview.
    if('__no_secret_because_typst-preview_doesnt_support_it__' === secret) {
        return getUnsafeSocketCompat(url);
    }

    return new Promise((wsresolve) => {
        let _sock: WebSocket | undefined = undefined;

        let prews = webSocket<ArrayBuffer>({
            url,
            binaryType: "arraybuffer",
            serializer: t => t,
            deserializer: (event) => event.data,
            openObserver: {
                next: (e) => {
                    console.log('WebSocket connection opened', e.target);
                    _sock = e.target as any;
                }
            },
        });

        // Dummy to keep websocket alive :)
        prews.subscribe({
            error: () => {}, // Ignore errors for this subscriber
        }); 

        const challengeForServer = generateCryptoRandom(32);

        // Auth stage 1: We authenticate to the server
        const stage1Subscription = {
            next: async (data: ArrayBuffer) => {
                const message = JSON.parse(dec.decode(data));
                if(message.challenge === undefined)
                    throw new Error("Missing challenge.");

                const cnonce = generateCryptoRandom(32);
                prews.next(enc.encode(JSON.stringify({
                    'cnonce': cnonce,
                    'hash': await digestHex(secret + ":" + message.challenge + ":" + cnonce),
                    'challenge': challengeForServer
                })));

                // Continue to stage 2
                subscription.unsubscribe();
                subscription = prews.subscribe(stage2Subscription);

            },
            error: () => {
                // TODO: Is it really good to retry here?
                console.log("WebSocket err during auth stage 1, will retry");
                setTimeout(() => {
                    getAuthenticatedSocket(url, secret, dec, enc).then(x => wsresolve(x))
                }, 1000);
            },
        };

        // Auth stage 2: The server authenticates to us
        const stage2Subscription = {
            next: async (data: ArrayBuffer) => {
                const message = JSON.parse(dec.decode(data));

                if(message.auth !== undefined) {
                    // Server didn't like our 'hash'
                    // Probably we have an outdated secret (when tinymist preview was restarted)
                    // TODO: How to make this a nice user-facing error?
                    throw new Error("Wrong or outdated secret");
                }

                // Server liked our 'hash'. Now we check if the server is malicious or not
                if(message.snonce === undefined || message.hash === undefined)
                    throw new Error("Missing snonce or hash.");
                if(message.hash !== await digestHex(secret + ":" + challengeForServer + ":" + message.snonce))
                    throw new Error("Malicious server detected?!");

                // Authentication succeeded!
                console.log("Authentication succeeded");
                subscription.unsubscribe();
                wsresolve({ websocketSubject: prews, websocket: _sock });
            }
        }

        // We start with stage 1
        let subscription = prews.subscribe(stage1Subscription);
    });
}
