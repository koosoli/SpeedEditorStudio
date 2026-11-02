// Small line icons (24×24, drawn with the current text colour), so buttons
// can show an icon with a tooltip instead of a word. Inline SVG: no files,
// no network, works offline.

const ICON_PATHS = {
  // top bar
  learn: "M12 2v4M12 18v4M2 12h4M18 12h4M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
  import: "M12 3v12M7 10l5 5 5-5M4 17v3h16v-3",
  export: "M12 15V3M7 8l5-5 5 5M4 17v3h16v-3",
  revert: "M4 9h11a5 5 0 0 1 0 10H8M4 9l4-4M4 9l4 4",
  save: "M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6",
  gear: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z",
  plug: "M9 2v6M15 2v6M6 8h12v3a6 6 0 0 1-12 0zM12 17v5",
  bolt: "M13 2L4 14h7l-1 8 9-12h-7z",
  // action kinds
  none: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM5.6 5.6l12.8 12.8",
  davinci: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10zM12 11a1 1 0 1 0 0 2 1 1 0 0 0 0-2z",
  keys: "M3 6h18v12H3zM7 10h.01M11 10h.01M15 10h.01M7 14h10",
  resolve: "M3 7h18v13H3zM3 7l3-4h3l-3 4M10 3h3l-3 4M15 3h3l-3 4",
  text: "M5 6V4h14v2M12 4v16M9 20h6",
  run: "M14 4h6v6M20 4L10 14M18 14v6H4V6h6",
  menu: "M4 6h16M4 12h16M4 18h10",
  script: "M8 8l-4 4 4 4M16 8l4 4-4 4M14 5l-4 14",
  chevron: "M9 6l6 6-6 6",
  // views and layers
  picture: "M3 6h18v12H3zM7 10h3M7 14h3M16 12m-2.5 0a2.5 2.5 0 1 0 5 0 2.5 2.5 0 1 0-5 0",
  list: "M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01",
  layer: "M12 3l9 5-9 5-9-5zM3 13l9 5 9-5",
  tracks: "M3 7h18M3 12h18M3 17h18M7 5v4M12 10v4M17 15v4",
};

/** An inline SVG icon by name (stroke icons in the current text colour). */
function icon(name, size = 16) {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", size);
  svg.setAttribute("height", size);
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.8");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("icon");
  const path = document.createElementNS(ns, "path");
  path.setAttribute("d", ICON_PATHS[name] || ICON_PATHS.none);
  svg.append(path);
  return svg;
}

/** Puts icons into elements marked with data-icon="name". */
function applyIcons(root = document) {
  for (const el of root.querySelectorAll("[data-icon]")) {
    if (!el.querySelector(":scope > svg.icon")) el.prepend(icon(el.dataset.icon));
  }
}
