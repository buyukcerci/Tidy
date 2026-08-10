import { invoke } from "@tauri-apps/api/core";

export type ConnectionState = "no_account" | "disconnected" | "connected";

export type AuthenticationStatus =
  | "idle"
  | "connected"
  | "temporarily_unavailable"
  | "reauthentication_required";

export type Account = {
  subject_id: string;
  email: string | null;
  display_name: string | null;
  status: string;
};

export type ConnectionStatus = {
  oauth_configured: boolean;
  state: ConnectionState;
  account: Account | null;
  authentication: AuthenticationStatus;
  auth_message: string | null;
};

export type DisconnectOutcome = {
  revoked: boolean;
  keychain_cleared: boolean;
};

export function getConnectionStatus(): Promise<ConnectionStatus> {
  return invoke<ConnectionStatus>("get_connection_status");
}

export function saveOauthConfiguration(
  clientId: string,
  clientSecret: string
): Promise<void> {
  return invoke("save_oauth_configuration", { clientId, clientSecret });
}

export function beginGoogleConnect(): Promise<Account> {
  return invoke<Account>("begin_google_connect");
}

export function cancelGoogleConnect(): Promise<void> {
  return invoke("cancel_google_connect");
}

export function disconnectGoogleAccount(
  eraseLocalData: boolean
): Promise<DisconnectOutcome> {
  return invoke<DisconnectOutcome>("disconnect_google_account", {
    eraseLocalData,
  });
}

export type ParsedClientConfig = {
  clientId: string;
  clientSecret: string;
};

type ClientJson = {
  installed?: { client_id?: string; client_secret?: string };
  web?: { client_id?: string; client_secret?: string };
};

export function parseGoogleClientJson(
  text: string
): ParsedClientConfig | null {
  try {
    const parsed = JSON.parse(text) as ClientJson;
    const source = parsed.installed ?? parsed.web;
    const clientId = source?.client_id?.trim() ?? "";
    const clientSecret = source?.client_secret?.trim() ?? "";
    if (clientId.length >= 5 && clientSecret.length > 0) {
      return { clientId, clientSecret };
    }
    return null;
  } catch {
    return null;
  }
}