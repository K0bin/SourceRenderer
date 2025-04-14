https://issues.chromium.org/issues/41483010

https://bugzilla.mozilla.org/show_bug.cgi?id=1960229
Maybe related: https://bugzilla.mozilla.org/show_bug.cgi?id=1257440

Firefox doesn't support SharedArrayBuffer (backing store of the WASM memory when using threads)
for GPUQueue.writeTexture but the spec explicitly says that's allowed.
