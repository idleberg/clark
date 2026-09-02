// Stands in for `memfs` where the clack checkout has not installed it. Only ./setup.mjs imports it,
// and only to read the volume the `path` suite builds — no volume, nothing to write down.
export const vol = null;
