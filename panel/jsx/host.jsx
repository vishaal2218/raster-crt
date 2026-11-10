/*
 * Raster CRT host script (ExtendScript / ES3) for Premiere Pro.
 *
 * Every entry point takes one JSON string and returns a JSON envelope:
 *   { "ok": true, "data": ... }  or  { "ok": false, "error": { "code": "...", "detail": "..." } }
 * The panel translates the codes; raw host text is never shown.
 *
 * Controls are addressed by their Effect Controls name (the params.json label). Names are
 * unique across the effect, and Premiere also lists group headers, which are skipped.
 */

var RC = (function () {
  var EFFECT_NAME = 'Raster CRT';

  /* ---------- JSON (ExtendScript has no native JSON) ---------- */

  function quote(s) {
    var out = '"';
    for (var i = 0; i < s.length; i++) {
      var c = s.charAt(i);
      var code = s.charCodeAt(i);
      if (c === '"' || c === '\\') out += '\\' + c;
      else if (code < 32) out += '\\u' + ('0000' + code.toString(16)).slice(-4);
      else out += c;
    }
    return out + '"';
  }

  function stringify(v) {
    if (v === null || v === undefined) return 'null';
    var type = typeof v;
    if (type === 'number') return isFinite(v) ? String(v) : 'null';
    if (type === 'boolean') return v ? 'true' : 'false';
    if (type === 'string') return quote(v);
    var parts = [];
    var i;
    if (v instanceof Array) {
      for (i = 0; i < v.length; i++) parts.push(stringify(v[i]));
      return '[' + parts.join(',') + ']';
    }
    for (var k in v) {
      if (v.hasOwnProperty(k)) parts.push(quote(k) + ':' + stringify(v[k]));
    }
    return '{' + parts.join(',') + '}';
  }

  /* Input is only ever JSON produced by the panel's JSON.stringify. */
  function parse(s) {
    return eval('(' + s + ')');
  }

  function hostError(code, detail) {
    var e = new Error(detail || code);
    e.rcCode = code;
    return e;
  }

  function entry(fn) {
    return function (json) {
      try {
        return stringify({ ok: true, data: fn(json ? parse(json) : {}) });
      } catch (e) {
        return stringify({ ok: false, error: { code: e.rcCode || 'HOST', detail: String(e.message || e) } });
      }
    };
  }

  /* ---------- finding the clip and its Raster CRT instance ---------- */

  function activeSequence() {
    return app.project ? app.project.activeSequence : null;
  }

  function selectedVideoClips(seq) {
    var out = [];
    var selection = seq.getSelection();
    for (var i = 0; i < selection.length; i++) {
      if (selection[i].mediaType === 'Video') out.push(selection[i]);
    }
    return out;
  }

  function findEffect(clip) {
    var comps = clip.components;
    for (var i = 0; i < comps.numItems; i++) {
      if (comps[i].displayName === EFFECT_NAME) return comps[i];
    }
    return null;
  }

  function currentClip() {
    var seq = activeSequence();
    if (!seq) throw hostError('NO_SEQUENCE');
    var clips = selectedVideoClips(seq);
    if (!clips.length) throw hostError('NO_CLIP');
    return { seq: seq, clip: clips[0] };
  }

  function requireEffect() {
    var clip = currentClip().clip;
    var comp = findEffect(clip);
    if (!comp) throw hostError('NO_EFFECT', clip.name);
    return comp;
  }

  /* Group headers have no value; getValue throws or returns nothing for them. */
  function valueOf(prop) {
    try {
      var v = prop.getValue();
      return v === undefined ? null : v;
    } catch (e) {
      return null;
    }
  }

  function isKeyframed(prop) {
    try {
      return prop.isTimeVarying();
    } catch (e) {
      return false;
    }
  }

  function findControl(comp, label) {
    var props = comp.properties;
    for (var i = 0; i < props.numItems; i++) {
      if (props[i].displayName === label) return props[i];
    }
    throw hostError('NO_CONTROL', label);
  }

  /* ---------- entry points ---------- */

  function ping() {
    return { host: 'PPRO', version: app.version, effect: EFFECT_NAME };
  }

  function selection() {
    var seq = activeSequence();
    if (!seq) return { sequence: null, clip: null, count: 0, hasEffect: false };
    var clips = selectedVideoClips(seq);
    var clip = clips.length ? clips[0] : null;
    return {
      sequence: seq.name,
      clip: clip ? clip.name : null,
      clipId: clip ? clip.nodeId : null,
      count: clips.length,
      hasEffect: clip ? findEffect(clip) !== null : false
    };
  }

  function read() {
    var props = requireEffect().properties;
    var values = {};
    var keyframed = [];
    for (var i = 0; i < props.numItems; i++) {
      var v = valueOf(props[i]);
      if (v === null) continue;
      values[props[i].displayName] = v;
      if (isKeyframed(props[i])) keyframed.push(props[i].displayName);
    }
    return { values: values, keyframed: keyframed, entries: props.numItems };
  }

  /* The clip's own time at the playhead, which is where keyframes live. */
  function clipTimeAtPlayhead(found) {
    var t = new Time();
    t.seconds = found.seq.getPlayerPosition().seconds - found.clip.start.seconds + found.clip.inPoint.seconds;
    return t;
  }

  /* A keyframed control gets a keyframe at the playhead instead of a new static value. */
  function writeOne(comp, label, value, found) {
    var prop = findControl(comp, label);
    if (isKeyframed(prop)) {
      if (!found) throw hostError('KEYFRAMED', label);
      var t = clipTimeAtPlayhead(found);
      try {
        prop.addKey(t);
        prop.setValueAtKey(t, value, true);
      } catch (e) {
        throw hostError('KEYFRAMED', label);
      }
      return value;
    }
    prop.setValue(value, true);
    return valueOf(prop);
  }

  function write(args) {
    var found = currentClip();
    var comp = findEffect(found.clip);
    if (!comp) throw hostError('NO_EFFECT', found.clip.name);
    return { label: args.label, value: writeOne(comp, args.label, args.value, found) };
  }

  /* Writes several controls; keyframed ones are skipped and reported, not fatal. */
  function writeMany(args) {
    var comp = requireEffect();
    var skipped = [];
    for (var i = 0; i < args.values.length; i++) {
      try {
        writeOne(comp, args.values[i].label, args.values[i].value);
      } catch (e) {
        if (e.rcCode !== 'KEYFRAMED') throw e;
        skipped.push(args.values[i].label);
      }
    }
    return { written: args.values.length - skipped.length, skipped: skipped };
  }

  /* Finds the QE twin of a DOM clip by track and start time (QE lists gaps as items too). */
  function qeItemFor(seq, clip) {
    var qeSeq = qe.project.getActiveSequence();
    for (var t = 0; t < seq.videoTracks.numTracks; t++) {
      var track = seq.videoTracks[t];
      for (var c = 0; c < track.clips.numItems; c++) {
        if (track.clips[c].nodeId !== clip.nodeId) continue;
        var qeTrack = qeSeq.getVideoTrackAt(t);
        for (var q = 0; q < qeTrack.numItems; q++) {
          var item = qeTrack.getItemAt(q);
          if (item.type !== 'Empty' && String(item.start.ticks) === String(clip.start.ticks)) return item;
        }
      }
    }
    throw hostError('NO_QE_CLIP', clip.name);
  }

  function addEffect() {
    var found = currentClip();
    if (findEffect(found.clip)) return selection();
    app.enableQE();
    var fx = qe.project.getVideoEffectByName(EFFECT_NAME);
    if (!fx) throw hostError('NOT_INSTALLED');
    qeItemFor(found.seq, found.clip).addVideoEffect(fx);
    return selection();
  }

  return {
    ping: entry(ping),
    selection: entry(selection),
    read: entry(read),
    write: entry(write),
    writeMany: entry(writeMany),
    addEffect: entry(addEffect)
  };
}());
