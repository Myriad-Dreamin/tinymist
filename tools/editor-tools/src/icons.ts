import van from "vanjs-core";
const { div, span } = van.tags;

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

export const HelpIcon = (sz: number = 16) =>
  div({
    class: "tinymist-icon",
    style: `height: ${sz}px; width: ${sz}px;`,
    innerHTML: `<svg
    viewBox="0 0 24 24"
    preserveAspectRatio="xMidYMid meet"
    xmlns="http://www.w3.org/2000/svg"
  >
    <path
      class="stroke-based"
      d="M9.08997 9.00007C9.32507 8.33174 9.78912 7.76818 10.3999 7.40921C11.0107 7.05023 11.7289 6.91901 12.4271 7.03879C13.1254 7.15856 13.7588 7.5216 14.215 8.0636C14.6713 8.60561 14.921 9.2916 14.92 10.0001C14.92 12.0001 11.92 13.0001 11.92 13.0001M12 17.0001H12.01M3 7.94153V16.0586C3 16.4013 3 16.5726 3.05048 16.7254C3.09515 16.8606 3.16816 16.9847 3.26463 17.0893C3.37369 17.2077 3.52345 17.2909 3.82297 17.4573L11.223 21.5684C11.5066 21.726 11.6484 21.8047 11.7985 21.8356C11.9315 21.863 12.0685 21.863 12.2015 21.8356C12.3516 21.8047 12.4934 21.726 12.777 21.5684L20.177 17.4573C20.4766 17.2909 20.6263 17.2077 20.7354 17.0893C20.8318 16.9847 20.9049 16.8606 20.9495 16.7254C21 16.5726 21 16.4013 21 16.0586V7.94153C21 7.59889 21 7.42756 20.9495 7.27477C20.9049 7.13959 20.8318 7.01551 20.7354 6.91082C20.6263 6.79248 20.4766 6.70928 20.177 6.54288L12.777 2.43177C12.4934 2.27421 12.3516 2.19543 12.2015 2.16454C12.0685 2.13721 11.9315 2.13721 11.7985 2.16454C11.6484 2.19543 11.5066 2.27421 11.223 2.43177L3.82297 6.54288C3.52345 6.70928 3.37369 6.79248 3.26463 6.91082C3.16816 7.01551 3.09515 7.13959 3.05048 7.27477C3 7.42756 3 7.59889 3 7.94153Z"
      stroke-width="2"
      fill-rule="nonzero"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>`,
  });

export const ContributeIcon = (sz: number = 16, inline?: boolean) =>
  (inline ? span : div)({
    class: "tinymist-icon",
    style: `height: ${sz}px; width: ${sz}px;`,
    innerHTML: `<svg xmlns="http://www.w3.org/2000/svg" width="${sz}px" height="${sz}px" class="shrink-0 w-5 h-5 inline align-text-top" role="img" aria-label="fire solid" viewBox="0 0 24 24"><path d="M8.597 3.2A1 1 0 0 0 7.04 4.289a3.49 3.49 0 0 1 .057 1.795 3.448 3.448 0 0 1-.84 1.575.999.999 0 0 0-.077.094c-.596.817-3.96 5.6-.941 10.762l.03.049a7.73 7.73 0 0 0 2.917 2.602 7.617 7.617 0 0 0 3.772.829 8.06 8.06 0 0 0 3.986-.975 8.185 8.185 0 0 0 3.04-2.864c1.301-2.2 1.184-4.556.588-6.441-.583-1.848-1.68-3.414-2.607-4.102a1 1 0 0 0-1.594.757c-.067 1.431-.363 2.551-.794 3.431-.222-2.407-1.127-4.196-2.224-5.524-1.147-1.39-2.564-2.3-3.323-2.788a8.487 8.487 0 0 1-.432-.287Z"></path></svg>`,
  });

export const AddIcon = (sz: number = 16) =>
  div({
    class: "tinymist-icon",
    style: `height: ${sz}px; width: ${sz}px;`,
    innerHTML: `<svg width="${sz}px" height="${sz}px" viewBox="-1 0 17 17">
  <path d="M7.75 2a.75.75 0 0 1 .75.75V7h4.25a.75.75 0 0 1 0 1.5H8.5v4.25a.75.75 0 0 1-1.5 0V8.5H2.75a.75.75 0 0 1 0-1.5H7V2.75A.75.75 0 0 1 7.75 2Z"></path>
</svg>`,
  });

export const CopyIcon = (sz: number = 16) =>
  div({
    class: "tinymist-icon",
    style: `height: ${sz}px; width: ${sz}px;`,
    innerHTML: `<svg width="${sz}px" height="${sz}px" viewBox="0 0 16 16" version="1.1"
  xmlns="http://www.w3.org/2000/svg"
  xmlns:xlink="http://www.w3.org/1999/xlink">
  <g stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
    <rect class="stroke-based" width="9.8202543" height="11.792212" x="1.742749" y="3.4055943" ry="0.49967012" />
    <path class="stroke-based" d="m 5.1841347,0.82574918 9.0495613,0.0341483 V 12.129165" />
    <path class="stroke-based" d="M 3.6542046,6.2680732 H 9.3239071" />
    <path class="stroke-based" d="M 3.6542046,12.48578 H 7.7302609" />
    <path class="stroke-based" d="M 3.6542046,9.3769264 H 7.7302609" />
  </g>
</svg>`,
  });
