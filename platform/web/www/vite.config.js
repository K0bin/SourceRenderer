import wasm from "vite-plugin-wasm";

/**
 * @type {import('vite').UserConfig}
 */
const config = {
    build: {
        outDir: "dist",
        target: "esnext"
    },
    worker: {
        format: "es",
        plugins: () => {
            return [
                wasm(),
            ];
        }
    },
    plugins: [
        wasm(),
    ]
  }

export default config
