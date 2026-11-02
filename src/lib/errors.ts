/** Rust commands reject with a string. Anything else is a bug on our side, so keep its text. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string" && error.trim() !== "") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong. The app did not give a reason.";
}
