import van from "vanjs-core";
const { div } = van.tags;

export const HeartIcon = (sz: number = 16) =>
  div({
    class: "tinymist-icon",
    style: `height: ${sz}px; width: ${sz}px;`,
    innerHTML: `<svg width="${sz}px" height="${sz}px" viewBox="0 0 16 16" version="1.1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
<g stroke="none" stroke-width="1" fill="none" fill-rule="evenodd">
  <path d="M10.8049818,3 C8.78471579,3 8.00065285,5.34486486 8.00065285,5.34486486 C8.00065285,5.34486486 7.21296387,3 5.19604494,3 C3.49431318,3 1.748374,4.09592694 2.03008996,6.51430532 C2.37372765,9.46673775 7.75491917,12.9928738 7.99310958,13.0010557 C8.23129998,13.0092378 13.7309828,9.2785378 13.981459,6.5012405 C14.1878647,4.20097023 12.5067136,3 10.8049818,3 Z"/>
</g>
</svg>`,
  });

export const AddIcon = (sz: number = 16) =>
  div({
    class: "tinymist-icon",
    style: `height: ${sz}px; width: ${sz}px;`,
    innerHTML: `<svg width="${sz}px" height="${sz}px" viewBox="-1 0 17 17">
  <path d="M7.75 2a.75.75 0 0 1 .75.75V7h4.25a.75.75 0 0 1 0 1.5H8.5v4.25a.75.75 0 0 1-1.5 0V8.5H2.75a.75.75 0 0 1 0-1.5H7V2.75A.75.75 0 0 1 7.75 2Z"></path>
</svg>`,
  });
