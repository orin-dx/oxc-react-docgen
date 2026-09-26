import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    pool: 'threads',
    include: ['tests/unit/**/*.test.ts'],
    coverage: { provider: 'v8', include: ['src/**/*.ts'], reporter: ['text', 'lcov'] },
  },
})
