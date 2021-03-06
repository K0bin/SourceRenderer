const CopyWebpackPlugin = require("copy-webpack-plugin");
const Path = require('path');
const WorkboxPlugin = require('workbox-webpack-plugin');


module.exports = {
  entry: "./bootstrap.js",
  output: {
    path: Path.resolve(__dirname, "dist"),
    filename: "bundle.js",
  },
  module: {
    rules: [
      {
        test: /\.tsx?$/,
        use: 'ts-loader',
        exclude: /node_modules/,
      },
    ]
  },
  resolve: {
    extensions: ['.tsx', '.ts', '.js'],
  },
  plugins: [
    new CopyWebpackPlugin({ patterns: ['index.html'] }),
    new WorkboxPlugin.GenerateSW({
      clientsClaim: true,
      skipWaiting: true,
    })
  ],
  mode: 'development',
  experiments:  {
    syncWebAssembly: true
  }
};
