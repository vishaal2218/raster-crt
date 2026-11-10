// The preset browser: factory looks by group, then the user's own, as rows of
// [thumbnail] [NAME] [group]. Two columns from 440 px wide, like the rack.
import { renderIcons } from './icons.js';

export const GROUPS = Object.freeze([['camera', 'Camera looks'], ['tv', 'TV looks'], ['glitch', 'Glitch']]);
const PERSONAL_THUMB = 'linear-gradient(160deg, var(--accent-dim), var(--surface-sunken))';

function heading(text) {
  const h = document.createElement('h2');
  h.className = 'section-label';
  h.textContent = text;
  return h;
}

function tile({ name, tag, thumb, selected, onPick }) {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'tile';
  button.setAttribute('role', 'option');
  button.setAttribute('aria-selected', String(selected));
  const art = thumb.startsWith('url(') || thumb.startsWith('linear') ? document.createElement('span') : document.createElement('img');
  art.className = 'tile-thumb';
  if (art.tagName === 'IMG') {
    art.src = thumb;
    art.alt = '';
  } else {
    art.style.background = thumb;
  }
  const label = document.createElement('span');
  label.innerHTML = '<div class="tile-name"></div><div class="tile-tag"></div>';
  label.querySelector('.tile-name').textContent = name;
  label.querySelector('.tile-tag').textContent = tag;
  button.append(art, label);
  button.addEventListener('click', onPick);
  return button;
}

/**
 * @param {object} o
 * @param {object[]} o.factory            factory.json presets
 * @param {(look: object, index: number) => void} o.onPickFactory   index is 0-based
 * @param {(look: object) => void} o.onPickPersonal
 * @param {(look: object) => void} o.onDeletePersonal
 */
export function createBrowser({ factory, onPickFactory, onPickPersonal, onDeletePersonal }) {
  const root = document.createElement('div');
  root.className = 'browser';
  root.setAttribute('role', 'listbox');
  root.setAttribute('aria-label', 'Presets');

  function render(personal, active) {
    root.replaceChildren();
    for (const [category, title] of GROUPS) {
      const grid = document.createElement('div');
      grid.className = 'tile-grid';
      factory.forEach((look, index) => {
        if (look.category !== category) return;
        grid.append(tile({
          name: look.name, tag: title.replace(' looks', ''), thumb: `presets/thumbnails/${look.thumbnail}`,
          selected: active?.kind === 'factory' && active.name === look.name,
          onPick: () => onPickFactory(look, index),
        }));
      });
      root.append(heading(title), grid);
    }
    root.append(heading('My presets'));
    if (!personal.length) {
      const empty = document.createElement('div');
      empty.className = 'empty-note';
      empty.textContent = 'Save a look and it lands here.';
      root.append(empty);
      return;
    }
    const grid = document.createElement('div');
    grid.className = 'tile-grid';
    personal.forEach((look) => {
      const row = document.createElement('div');
      row.className = 'tile-row';
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'key is-icon tile-delete';
      remove.setAttribute('aria-label', `Delete ${look.name}`);
      remove.innerHTML = '<i data-lucide="trash"></i>';
      remove.addEventListener('click', () => onDeletePersonal(look));
      row.append(tile({
        name: look.name, tag: 'Mine', thumb: PERSONAL_THUMB,
        selected: active?.kind === 'personal' && active.name === look.name,
        onPick: () => onPickPersonal(look),
      }), remove);
      grid.append(row);
    });
    root.append(grid);
    renderIcons(root);
  }

  return { el: root, render };
}
