import { defineConfig } from 'vitest/config'

// Layer 2 (see tests/integration/plugin.integration.test.ts's own header
// comment). pool: 'forks' is required — the real native .node binary cannot
// load inside vitest's default worker-thread pool.
export default defineConfig({
  test: {
    pool: 'forks',
    include: ['tests/integration/**/*.test.ts'],
    testTimeout: 15000,
  },
})
