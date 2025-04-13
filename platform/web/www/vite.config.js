import wasm from "vite-plugin-wasm";

/** @type {import('vite').UserConfig} */
export default {
    build: {
        outDir: "dist",
        target: "esnext",
        rollupOptions: {
            input: {
                site: "src/main.ts",
                rust_glue: "src/web_glue.ts",
            },
        },
    },
    worker: {
        format: "es",
    },
    server: {
        headers: {
          "Cross-Origin-Embedder-Policy": "require-corp",
          "Cross-Origin-Opener-Policy": "same-origin",
        },
        fs: {
            strict: false,
            allow: [
                '../lib/pkg',
            ],
        },
    },
    appType: "spa",
};
