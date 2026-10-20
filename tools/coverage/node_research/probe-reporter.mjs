import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { captureReporter } from "./capture.mjs";
export function probeReporter(destination, includeTests, workspace) {
  return {
    onInit(context) {
      captureReporter(destination).onInit(context);
      const provider = context.coverageProvider;
      const previous = provider.isIncluded.bind(provider);
      const decisions = [];
      provider.isIncluded = (filename) => {
        const native = previous(filename);
        const selected =
          includeTests &&
          filename.startsWith(workspace) &&
          !filename.includes("/node_modules/");
        decisions.push({ filename, native, selected });
        return selected || native;
      };
      const generate = provider.generateCoverage.bind(provider);
      provider.generateCoverage = async (...args) => {
        const result = await generate(...args);
        writeFileSync(
          join(destination, "istanbul.json"),
          JSON.stringify(result.toJSON(), null, 2),
        );
        writeFileSync(
          join(destination, "inclusion-decisions.json"),
          JSON.stringify(decisions, null, 2),
        );
        return result;
      };
    },
  };
}
