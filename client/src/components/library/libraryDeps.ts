/** The real library behind the import pipeline. */

import { addFile, inspect } from "../../lib/library";
import type { ImportDeps } from "./importFiles";

/** Imports into the local library, verifying with the engine. */
export const libraryImportDeps: ImportDeps = {
  add: addFile,
  inspect: async (bytes) => {
    const { bundle, summary, report } = await inspect(bytes);
    bundle.free();
    return { title: summary.title, report };
  },
};
