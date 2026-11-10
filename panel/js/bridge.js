// Promise wrapper around CEP's evalScript for the RC.* entry points in jsx/host.jsx.
// Host failures arrive as BridgeError(code, detail) so the panel can show its own words.

export const ENTRY_POINTS = Object.freeze(['ping', 'selection', 'read', 'write', 'writeMany', 'addEffect']);

export class BridgeError extends Error {
  constructor(code, detail = '') {
    super(detail ? `${code}: ${detail}` : code);
    this.code = code;
    this.detail = detail;
  }
}

/** A JSON string literal that is safe inside ExtendScript (ES3) source. */
export function scriptLiteral(value) {
  return JSON.stringify(JSON.stringify(value ?? {}))
    .split(' ').join('\\u2028')
    .split(' ').join('\\u2029');
}

export function parseEnvelope(raw) {
  if (typeof raw !== 'string' || raw === 'EvalScript error.') throw new BridgeError('BRIDGE', String(raw));
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch {
    throw new BridgeError('BRIDGE', raw.slice(0, 200));
  }
  if (parsed?.ok) return parsed.data;
  throw new BridgeError(parsed?.error?.code || 'HOST', parsed?.error?.detail || '');
}

export function createBridge(cep) {
  const evalRaw = (script) => new Promise((resolve) => cep.evalScript(script, resolve));

  async function call(name, args = {}) {
    if (!ENTRY_POINTS.includes(name)) throw new BridgeError('BRIDGE', `unknown entry point ${name}`);
    return parseEnvelope(await evalRaw(`RC.${name}(${scriptLiteral(args)})`));
  }

  /** Loads host.jsx again if the manifest's ScriptPath did not (or Premiere reset it). */
  async function ensureHostScript(hostScriptPath) {
    if ((await evalRaw('typeof RC')) === 'object') return;
    await evalRaw(`$.evalFile(${JSON.stringify(hostScriptPath)})`);
    await call('ping');
  }

  return { call, ensureHostScript };
}
