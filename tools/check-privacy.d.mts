import type { Buffer } from "node:buffer";
export interface PrivateValue {
  category: string;
  value: string;
}
export interface PrivacyFinding {
  path?: string;
  blob?: string;
  commit?: string;
  ref?: string;
  categories: string[];
}
export function inspectPrivacy(
  filePath: string,
  contents: Buffer,
  privateValues?: PrivateValue[],
): string[];
export function loadPrivateValues(settingsFile: string): PrivateValue[];
export function inspectGitIdentity(identity: string): string[];
export function checkRepository(options?: {
  repo?: string;
  settings?: string;
  history?: boolean;
  staged?: boolean;
}): { entriesChecked: number; findings: PrivacyFinding[] };
