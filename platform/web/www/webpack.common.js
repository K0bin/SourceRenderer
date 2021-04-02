const CopyWebpackPlugin = require("copy-webpack-plugin");
const Path = require('path');
const WorkboxPlugin = require('workbox-webpack-plugin');

module.exports = {
  stats: 'verbose',
  entry: {
    main: {
      import: "./bootstrap.js",
      filename: "main.bundle.js"
    }
  },
  output: {
    path: Path.resolve(__dirname, "dist"),
    clean: true
  },
  module: {
    rules: [
      {
        test: /\.wasm$/,
        type: 'webassembly/sync',
      },
      {
        test: /\.tsx?$/,
        use: {
          loader: 'ts-loader',
          options: {
            configFile: Path.resolve(__dirname, "tsconfig.json"),
            onlyCompileBundledFiles: true
          }
        },
        exclude: [
          /node_modules/,
          Path.resolve(__dirname, "src", "game_worker.ts"),
          Path.resolve(__dirname, "src", "asset_worker.ts"),
          Path.resolve(__dirname, "src", "service_worker.ts"),
        ]
      },
      {
        test: /\.tsx?$/,
        use: {
          loader: 'ts-loader',
          options: {
            configFile: Path.resolve(__dirname, "tsconfig.worker.json"),
            onlyCompileBundledFiles: true
          }
        },
        include: [
          Path.resolve(__dirname, "src", "game_worker.ts"),
          Path.resolve(__dirname, "src", "asset_worker.ts"),
          Path.resolve(__dirname, "src", "service_worker.ts"),
        ],
        enforce: 'pre'
      }
    ]
  },
  resolve: {
    extensions: ['.tsx', '.ts', '.js'],
  },
  plugins: [
    new CopyWebpackPlugin({ patterns: ['index.html', 'manifest.json'] }),
    new WorkboxPlugin.GenerateSW({
      // these options encourage the ServiceWorkers to get in there fast
      // and not allow any straggling "old" SWs to hang around
      clientsClaim: true,
      skipWaiting: true,

    }),
  ],
  experiments:  {
    syncWebAssembly: true
  }
};
