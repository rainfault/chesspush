import { invoke } from "@tauri-apps/api/core";

export interface AccountStatus {
  status:
    | "disconnected"
    | "connected"
    | "offline"
    | "authenticating"
    | "invalid_token"
    | "error";
  username: string | null;
  rapid: number | null;
  blitz: number | null;
  message: string | null;
  hasToken: boolean;
}

export const disconnectedAccount: AccountStatus = {
  status: "disconnected",
  username: null,
  rapid: null,
  blitz: null,
  message: null,
  hasToken: false,
};

export const refreshAccount = () => invoke<AccountStatus>("account_status");
export const connectToken = (token: string) =>
  invoke<AccountStatus>("connect_lichess_token", { token });
export const authenticate = () => invoke<AccountStatus>("start_lichess_auth");
export const cancelAuthentication = () =>
  invoke<AccountStatus>("cancel_lichess_auth");
export const disconnectAccount = () =>
  invoke<AccountStatus>("disconnect_lichess");
