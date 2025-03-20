import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    exclude: ["**/{node_modules,dist,test-dist}/**", "src/test/e2e/**"],
  },
});
