// import { ChildDom } from "vanjs-core";
import van from "vanjs-core";

const { button } = van.tags;

export function startModal(...contents: Node[]) {
  // mask window with a shadow and show message in floating window
  const shadow = document.createElement("div");
  shadow.style.position = "fixed";
  shadow.style.top = "0";
  shadow.style.left = "0";
  shadow.style.width = "100%";
  shadow.style.height = "100%";
  shadow.style.backgroundColor = "rgba(0, 0, 0, 0.5)";
  shadow.style.zIndex = "1000";
  document.body.appendChild(shadow);

  const floatingWindow = document.createElement("div");
  floatingWindow.classList.add("tinymist-window");
  floatingWindow.style.position = "fixed";
  floatingWindow.style.top = "50%";
  floatingWindow.style.left = "50%";
  floatingWindow.style.transform = "translate(-50%, -50%)";
  floatingWindow.style.width = "80%";
  floatingWindow.style.maxWidth = "800px";
  floatingWindow.style.height = "80%";
  floatingWindow.style.maxHeight = "600px";
  floatingWindow.style.backgroundColor = "var(--modal-background)";
  floatingWindow.style.padding = "1rem";
  floatingWindow.style.overflow = "auto";
  floatingWindow.style.zIndex = "1001";
  floatingWindow.style.borderRadius = "6px";

  // also shows close button and help
  // Press button/space/enter to close this window
  const close = button(
    {
      class: "tinymist-button",
    },
    "Close"
  );
  const keydownHandler = (e: KeyboardEvent) => {
    if (e.key === "Escape" || e.key === " " || e.key === "Enter") {
      removeModal();
    }
  };
  const removeModal = () => {
    document.body.removeChild(shadow);
    document.body.removeChild(floatingWindow);
    window.removeEventListener("keydown", keydownHandler);
  };

  close.onclick = removeModal;
  window.addEventListener("keydown", keydownHandler);

  floatingWindow.appendChild(close);
  const help = button(
    {
      class: "tinymist-button",
      style: "margin-left: 0.5em",
      title:
        "Click the close button or press esc/space/enter to close this window",
    },
    "Help"
  );
  help.onclick = () => {
    alert(
      "Click the close button or press esc/space/enter to close this window"
    );
  };
  floatingWindow.appendChild(help);

  for (const content of contents) {
    floatingWindow.appendChild(content);
  }
  document.body.appendChild(floatingWindow);
}
