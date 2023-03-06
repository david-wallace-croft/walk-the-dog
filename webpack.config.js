const path = require("path");
const CopyPlugin = require("copy-webpack-plugin");
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");

module.exports = {
  devServer: {
    allowedHosts: ['localhost'],
    client: {
      logging: 'verbose',
      overlay: true,
      progress: true,
    },
    open: true,
    static: false,
  },
  entry: {
    index: './js/entry-index.js'
  },
  experiments: {
    asyncWebAssembly: true,
  },
  mode: 'production',
  output: {
    filename: 'croftsoft-walk-the-dog.js',
    path: path.resolve(__dirname, 'dist')
  },
  plugins: [
    new CopyPlugin([
      path.resolve(__dirname, 'static')
    ]),
    new WasmPackPlugin({
      crateDirectory: __dirname,
    }),
  ]
};



// const dist = path.resolve(__dirname, "dist");

// module.exports = {
//   mode: "production",
//   entry: {
//     index: "./js/index.js"
//   },
//   output: {
//     path: dist,
//     filename: "[name].js"
//   },
//   devServer: {
//     contentBase: dist,
//   },
//   plugins: [
//     new CopyPlugin([
//       path.resolve(__dirname, "static")
//     ]),

//     new WasmPackPlugin({
//       crateDirectory: __dirname,
//     }),
//   ]
// };
