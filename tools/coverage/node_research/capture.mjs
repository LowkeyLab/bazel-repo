import { cpSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
export function captureReporter(destination) {
  return {
    onInit(context) {
      const provider = context.coverageProvider;
      const log = {
        providerPresent: Boolean(provider),
        sourceCaptures: 0,
        profileDirectoryPresent: false,
      };
      const save = () =>
        writeFileSync(
          join(destination, "capture-status.json"),
          JSON.stringify(log, null, 2),
        );
      if (!provider) {
        save();
        return;
      }
      const sources = provider.getSources.bind(provider);
      provider.getSources = async function (...args) {
        const result = await sources(...args);
        if (String(args[0]).includes("/projects/research/")) {
          const index = log.sourceCaptures++;
          writeFileSync(
            join(destination, `transform-${index}.json`),
            JSON.stringify(
              { url: args[0], code: result.code, map: result.map },
              null,
              2,
            ),
          );
          save();
        }
        return result;
      };
      log.remapCaptures = 0;
      const remap = provider.remapCoverage.bind(provider);
      provider.remapCoverage = async function (
        filename,
        wrapperLength,
        source,
        ...args
      ) {
        const index = log.remapCaptures++;
        writeFileSync(
          join(destination, `remap-${index}.json`),
          JSON.stringify(
            { filename, wrapperLength, code: source.code, map: source.map },
            null,
            2,
          ),
        );
        save();
        return remap(filename, wrapperLength, source, ...args);
      };
      const generate = provider.generateCoverage.bind(provider);
      provider.generateCoverage = async function (...args) {
        log.profileDirectoryPresent = existsSync(
          provider.coverageFilesDirectory,
        );
        if (log.profileDirectoryPresent)
          cpSync(
            provider.coverageFilesDirectory,
            join(destination, "provider-profiles"),
            { recursive: true },
          );
        save();
        return generate(...args);
      };
      save();
    },
  };
}
