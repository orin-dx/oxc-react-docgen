'use strict'

const { existsSync } = require('node:fs')
const { join } = require('node:path')

if (process.env.NAPI_RS_NATIVE_LIBRARY_PATH) {
  module.exports = require(process.env.NAPI_RS_NATIVE_LIBRARY_PATH)
} else {
  // Must match package name in crates/binding/Cargo.toml (oxc-react-docgen-napi → underscores)
  const binaryName = 'oxc_react_docgen_napi'
  const candidates = [
    join(__dirname, `${binaryName}.node`),
    join(__dirname, '../../target/release', `${binaryName}.node`),
    join(__dirname, '../../target/debug', `${binaryName}.node`),
  ]
  const found = candidates.find(p => existsSync(p))
  if (!found) {
    throw new Error(
      `@oxc-react-docgen/napi: native binary not found.\n` +
      `Run: cargo build -p oxc-react-docgen-napi\n` +
      `Searched:\n${candidates.map(p => `  ${p}`).join('\n')}`
    )
  }
  module.exports = require(found)
}
