// Personal looks: one JSON file, ~/Library/Application Support/Raster CRT/presets.json
// (format raster-crt-library/1). Saved to a temporary file and renamed into place, with the
// previous file kept as presets.json.bak, so a crash mid-save never loses the library.

export const LIBRARY_FORMAT = 'raster-crt-library/1';
export const PRESET_FORMAT = 'raster-crt-preset/1';
export const MAX_PRESETS = 500;
const NAME_MAX = 40;
const BAD_NAME = /[/\\:*?"<>|]/;

/** Why a name can't be used, in plain words, or null when it's fine. */
export function nameProblem(name) {
  const n = String(name ?? '').trim();
  if (!n) return 'Give the look a name.';
  if (n.length > NAME_MAX) return `Names can be up to ${NAME_MAX} characters.`;
  if (BAD_NAME.test(n)) return 'Names can\'t contain / \\ : * ? " < > |';
  return null;
}

const sameName = (a, b) => a.trim().toLowerCase() === b.trim().toLowerCase();

/**
 * @param {{fs: object, path: object, dir: string}} deps  Node's fs and path, and the folder
 */
export function createLibrary({ fs, path, dir }) {
  const file = path.join(dir, 'presets.json');

  function load() {
    if (!fs.existsSync(file)) return [];
    let doc;
    try {
      doc = JSON.parse(fs.readFileSync(file, 'utf8'));
    } catch (err) {
      throw new Error(`Your saved looks file can't be read (${err.message}). It has been left as it is.`);
    }
    if (doc?.format !== LIBRARY_FORMAT || !Array.isArray(doc.presets)) {
      throw new Error('Your saved looks file is in an unknown format. It has been left as it is.');
    }
    return doc.presets.filter((p) => p && typeof p.name === 'string' && p.values && typeof p.values === 'object');
  }

  function persist(presets) {
    fs.mkdirSync(dir, { recursive: true });
    const temp = `${file}.tmp`;
    fs.writeFileSync(temp, `${JSON.stringify({ format: LIBRARY_FORMAT, presets }, null, 2)}\n`);
    if (fs.existsSync(file)) fs.copyFileSync(file, `${file}.bak`);
    fs.renameSync(temp, file);
  }

  /** Saves a look; an existing look with the same name (ignoring case) is replaced. */
  function save(name, values, now = new Date()) {
    const problem = nameProblem(name);
    if (problem) throw new Error(problem);
    const entry = { format: PRESET_FORMAT, name: name.trim(), category: 'personal', created: now.toISOString(), values: { ...values } };
    const others = load().filter((p) => !sameName(p.name, entry.name));
    if (others.length >= MAX_PRESETS) throw new Error(`You can keep up to ${MAX_PRESETS} looks. Delete one first.`);
    const next = [...others, entry];
    persist(next);
    return next;
  }

  function remove(name) {
    const next = load().filter((p) => !sameName(p.name, name));
    persist(next);
    return next;
  }

  function exists(name) {
    return load().some((p) => sameName(p.name, name));
  }

  return { file, load, save, remove, exists };
}
