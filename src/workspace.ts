/** UI addresses only. Chess facts and PGN remain in the native repositories. */
export type WorkspaceAddress = {
  page: "statistics" | "repertoire" | "databases";
  label: string;
  section?: string;
  accountId?: string;
  gameId?: string;
  ply?: number;
  runId?: string;
  documentId?: string;
  chapterId?: string;
  nodeId?: string;
};
const key = "chesspush.workspace.v1";
export function readWorkspace(): WorkspaceAddress | null {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key) ?? "null");
    if (!value || typeof value !== "object") return null;
    const address = value as WorkspaceAddress;
    if (
      !["statistics", "repertoire", "databases"].includes(address.page) ||
      typeof address.label !== "string" ||
      !address.label.trim()
    )
      return null;
    return address;
  } catch {
    return null;
  }
}
export function saveWorkspace(address: WorkspaceAddress) {
  try {
    localStorage.setItem(key, JSON.stringify(address));
  } catch {
    /* Navigation works without storage. */
  }
}
