import { defineConfig } from 'vitest/config';

/**
 * Pure-logic unit tests for the SPA (there is no jsdom component harness yet).
 * Run with `npm test`; CI runs it right after `npm ci`.
 */
export default defineConfig({
  test: {
    include: ['src/**/*.test.{ts,tsx}'],
    environment: 'node',
  },
});
