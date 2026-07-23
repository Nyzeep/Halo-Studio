import { TargetRegistry } from "../src/index.js";

new TargetRegistry();

// Test-only filesystem race hooks must not be accepted through the package API.
// @ts-expect-error TargetRegistry has a zero-argument production constructor.
new TargetRegistry({ readHooks: { afterOpen: async () => undefined } });
