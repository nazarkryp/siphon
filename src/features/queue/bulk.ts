import type { Download } from "../../api";
import { errorMessage } from "../../lib/errors";

export interface BulkFailure {
  download: Download;
  message: string;
}

/** Runs the call for each download in turn. A failure does not stop the others. */
export async function runEach(
  downloads: Download[],
  call: (download: Download) => Promise<void>,
): Promise<BulkFailure[]> {
  const failures: BulkFailure[] = [];
  for (const download of downloads) {
    try {
      await call(download);
    } catch (error) {
      failures.push({ download, message: errorMessage(error) });
    }
  }
  return failures;
}

/** One message for all failures: "Could not pause 2 of 5 downloads.", then one line per download. */
export function failureText(verb: string, failures: BulkFailure[], total: number): string {
  if (total === 1) return failures[0].message;
  const lines = failures.map((f) => `${f.download.fileName}: ${f.message}`);
  return [`Could not ${verb} ${failures.length} of ${total} downloads.`, ...lines].join("\n");
}
