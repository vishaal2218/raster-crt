// Raster CRT's own line icons (24 x 24, drawn with the current text colour). Replaces every
// <i data-lucide="name"> placeholder with an inline SVG, so the panel needs no network.

const PATHS = {
  'chevron-left': 'M15 18l-6-6 6-6',
  'chevron-right': 'M9 18l6-6-6-6',
  x: 'M18 6L6 18M6 6l12 12',
  check: 'M20 6L9 17l-5-5',
  plus: 'M12 5v14M5 12h14',
  minus: 'M5 12h14',
  diamond: 'M12 3l9 9-9 9-9-9z',
  trash: 'M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3',
  palette: 'M12 3a9 9 0 1 0 0 18c1.4 0 2-.9 2-1.9 0-1.1-1-1.5-1-2.6 0-1 .9-1.5 1.9-1.5H17a4 4 0 0 0 4-4c0-4.4-4-8-9-8zM7.5 11h.01M10 7h.01M15 7h.01M17 11h.01',
  zap: 'M13 2L4 14h7l-1 8 9-12h-7z',
  'triangle-alert': 'M12 3l10 18H2zM12 10v4M12 17.5v.5',
  sparkles: 'M11 4l1.8 5.2L18 11l-5.2 1.8L11 18l-1.8-5.2L4 11l5.2-1.8zM19 3v4M17 5h4',
  waves: 'M2 7c2.5 0 2.5-2 5-2s2.5 2 5 2 2.5-2 5-2 2.5 2 5 2M2 12c2.5 0 2.5-2 5-2s2.5 2 5 2 2.5-2 5-2 2.5 2 5 2M2 17c2.5 0 2.5-2 5-2s2.5 2 5 2 2.5-2 5-2 2.5 2 5 2',
  monitor: 'M4 4h16a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM8 21h8M12 17v4',
  camera: 'M4 8h3l2-3h6l2 3h3a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1zM12 10a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7z',
  crosshair: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 2v5M12 17v5M2 12h5M17 12h5',
  aperture: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM14.3 3.3L9 12.4M19.8 8.6H9.3M18.6 17l-5.3-9.1M9.7 20.7l5.3-9.1M4.2 15.4h10.5M5.4 7l5.3 9.1',
};

const SVG_NS = 'http://www.w3.org/2000/svg';

export function icon(name) {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '2');
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  svg.setAttribute('aria-hidden', 'true');
  const path = document.createElementNS(SVG_NS, 'path');
  path.setAttribute('d', PATHS[name] || PATHS.diamond);
  svg.append(path);
  return svg;
}

/** Swaps every icon placeholder under `root` for its SVG, keeping its classes and label. */
export function renderIcons(root = document) {
  root.querySelectorAll('i[data-lucide]').forEach((el) => {
    const svg = icon(el.getAttribute('data-lucide'));
    if (el.className) svg.setAttribute('class', el.className);
    const label = el.getAttribute('aria-label');
    if (label) {
      svg.setAttribute('aria-hidden', 'false');
      svg.setAttribute('role', 'img');
      svg.setAttribute('aria-label', label);
    }
    el.replaceWith(svg);
  });
}

export const ICON_NAMES = Object.freeze(Object.keys(PATHS));
