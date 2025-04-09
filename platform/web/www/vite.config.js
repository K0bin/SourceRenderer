import wasm from "vite-plugin-wasm";

/** @type {import('vite').UserConfig} */
export default {
    build: {
        outDir: "dist",
        target: "esnext"
    },
    worker: {
        format: "es",
    },
    server: {
        fs: {
            strict: false,
            allow: [
                '../lib/pkg',
            ],
        },
    },
    appType: "spa",
};
