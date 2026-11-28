import "@testing-library/jest-dom/vitest";

import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";

// Testing Library's default 1s limit for findBy*/waitFor is too tight when
// jsdom workers run in parallel on a loaded machine (CI, concurrent cargo
// builds): async renders finish correctly but occasionally after 1s. Waits
// still resolve as soon as their condition holds, so only genuine failures
// take longer to report. Keep this below Vitest's testTimeout.
configure({ asyncUtilTimeout: 5000 });

afterEach(() => {
  cleanup();
});
