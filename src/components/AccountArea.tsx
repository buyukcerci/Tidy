import { useState } from "react";
import type { AuthenticationStatus, DisconnectOutcome } from "../lib/oauth";

type ConnectionControls = {
  connecting: boolean;
  connectError: string | null;
  disconnecting: boolean;
  connect: () => void;
  cancelConnect: () => void;
  disconnect: (eraseLocalData: boolean) => Promise<DisconnectOutcome | null>;
};

type Props = {
  configured: boolean;
  connectedAccountName: string | null;
  disconnected: boolean;
  authentication: AuthenticationStatus;
  authMessage: string | null;
  controls: ConnectionControls;
  onOpenSetup: () => void;
  onDisconnectResult: (outcome: DisconnectOutcome | null) => void;
};

export function AccountArea({
  configured,
  connectedAccountName,
  disconnected,
  authentication,
  authMessage,
  controls,
  onOpenSetup,
  onDisconnectResult,
}: Props) {
  const [confirmingDisconnect, setConfirmingDisconnect] = useState(false);

  if (!configured) {
    return (
      <button className="account-state account-state-action" type="button" onClick={onOpenSetup}>
        Set up Google sign-in
      </button>
    );
  }

  if (controls.connecting) {
    return (
      <span className="account-state account-state-waiting">
        Waiting for approval in your browser…
      </span>
    );
  }

  if (connectedAccountName && authentication === "temporarily_unavailable") {
    return (
      <span className="account-state account-state-waiting">
        Drive unavailable — will retry automatically
      </span>
    );
  }

  if (connectedAccountName) {
    const initial = connectedAccountName.charAt(0).toUpperCase();
    return (
      <div className="account-state-account">
        <span className="account-avatar" aria-hidden="true">{initial}</span>
        <span className="account-label">
          <strong>{connectedAccountName}</strong>
          <span>Google Drive connected</span>
        </span>
        <span className="account-divider" />
        <span className="account-actions">
          <button
            type="button"
            className="account-link"
            onClick={() => setConfirmingDisconnect(true)}
          >
            Disconnect
          </button>
        </span>
        {confirmingDisconnect ? (
          <DisconnectDialog
            busy={controls.disconnecting}
            onCancel={() => setConfirmingDisconnect(false)}
            onChoice={async (erase) => {
              setConfirmingDisconnect(false);
              onDisconnectResult(await controls.disconnect(erase));
            }}
          />
        ) : null}
      </div>
    );
  }

  if (disconnected) {
    return (
      <span className="account-state">
        <span className="account-state-dot" />
        <span>{authentication === "reauthentication_required" || authMessage
          ? "Sign-in expired"
          : "Drive disconnected"}</span>
        <button type="button" className="account-link" onClick={controls.connect}>
          Reconnect
        </button>
      </span>
    );
  }

  return (
    <>
      {controls.connectError ? (
        <span className="account-state account-state-error" role="alert">
          {controls.connectError}
        </span>
      ) : (
        <button className="account-state account-state-action" type="button" onClick={controls.connect}>
          Connect Google Drive
        </button>
      )}
    </>
  );
}

function DisconnectDialog({
  busy,
  onCancel,
  onChoice,
}: {
  busy: boolean;
  onCancel: () => void;
  onChoice: (eraseLocalData: boolean) => void;
}) {
  return (
    <div className="modal-backdrop" role="presentation">
      <div className="modal modal-narrow" role="dialog" aria-modal="true" aria-labelledby="disconnect-title">
        <div className="modal-header">
          <div>
            <span className="modal-kicker">Account access</span>
            <h2 id="disconnect-title">Disconnect Drive?</h2>
          </div>
          <button className="icon-button" type="button" onClick={onCancel} aria-label="Close">
            <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" /></svg>
          </button>
        </div>

        <p className="modal-hint">
          Credentials are always removed from the system keychain. You can keep or erase the local
          scan data and history.
        </p>

        <button
          className="choice-row"
          type="button"
          disabled={busy}
          onClick={() => onChoice(false)}
        >
          <span className="choice-icon keep-icon" aria-hidden="true">01</span>
          <span><strong>Keep local data</strong><small>Tidy stays ready for this account to reconnect.</small></span>
        </button>

        <button
          className="choice-row choice-row-danger"
          type="button"
          disabled={busy}
          onClick={() => onChoice(true)}
        >
          <span className="choice-icon erase-icon" aria-hidden="true">02</span>
          <span><strong>Erase local data</strong><small>Remove the account record and all local scan data.</small></span>
        </button>

        <div className="modal-actions">
          <button className="secondary-action" type="button" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
