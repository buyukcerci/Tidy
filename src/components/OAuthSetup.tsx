import { useState } from "react";
import { parseGoogleClientJson, saveOauthConfiguration } from "../lib/oauth";

type Props = {
  onSaved: () => void;
  onClose: () => void;
};

export function OAuthSetup({ onSaved, onClose }: Props) {
  const [clientJson, setClientJson] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function handleSave() {
    const parsed = parseGoogleClientJson(clientJson);
    if (!parsed) {
      setError(
        "That doesn't look like a Google OAuth desktop client file. Paste the full contents of your downloaded client_secret.json."
      );
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await saveOauthConfiguration(parsed.clientId, parsed.clientSecret);
      onSaved();
    } catch (saveError) {
      setError(String(saveError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" role="presentation">
      <div className="modal oauth-setup-modal" role="dialog" aria-modal="true" aria-labelledby="oauth-setup-title">
        <div className="modal-header oauth-setup-header">
          <div className="oauth-heading">
            <span className="oauth-mark" aria-hidden="true">
              <svg viewBox="0 0 24 24" fill="none">
                <path d="M8 11V8a4 4 0 0 1 8 0v3" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
                <rect x="5" y="11" width="14" height="10" rx="3" stroke="currentColor" strokeWidth="1.8" />
                <circle cx="12" cy="16" r="1.3" fill="currentColor" />
              </svg>
            </span>
            <div>
              <span className="modal-kicker">Private connection</span>
              <h2 id="oauth-setup-title">Bring your own Google client</h2>
            </div>
          </div>
          <button className="icon-button" type="button" onClick={onClose} aria-label="Close">
            <svg viewBox="0 0 20 20" aria-hidden="true">
              <path d="m5 5 10 10M15 5 5 15" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
            </svg>
          </button>
        </div>

        <p className="oauth-intro">
          Tidy connects directly from this device. Your Google configuration stays local and your
          refresh token is stored in the system keychain.
        </p>

        <ol className="setup-steps">
          <li><span>1</span><div><strong>Create a client</strong><small>Choose Desktop app in Google Cloud Console.</small></div></li>
          <li><span>2</span><div><strong>Download JSON</strong><small>Download the generated client configuration.</small></div></li>
          <li><span>3</span><div><strong>Paste it below</strong><small>Tidy validates it before saving locally.</small></div></li>
        </ol>

        <div className={`config-field ${clientJson ? "has-value" : ""}`}>
          <div className="config-field-heading">
            <label className="field-label" htmlFor="client-json">Client configuration</label>
            <span>JSON</span>
          </div>
          <textarea
            id="client-json"
            className="text-area"
            value={clientJson}
            onChange={(event) => setClientJson(event.target.value)}
            placeholder='Paste the contents of client_secret.json here'
            spellCheck={false}
          />
        </div>

        {error ? <p className="field-error" role="alert">{error}</p> : null}

        <div className="scope-note">
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M10 3.2 16 5.7v4.1c0 3.6-2.5 6.1-6 7.2-3.5-1.1-6-3.6-6-7.2V5.7l6-2.5Z" stroke="currentColor" strokeWidth="1.5" fill="none" /><path d="m7.4 10 1.7 1.7 3.6-3.8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" /></svg>
          <span>Metadata only. Tidy never downloads or reads your file contents.</span>
        </div>

        <div className="modal-actions">
          <button className="secondary-action" type="button" onClick={onClose}>
            Not now
          </button>
          <button
            className="modal-primary-action"
            type="button"
            onClick={handleSave}
            disabled={busy || clientJson.trim().length === 0}
          >
            {busy ? "Saving..." : "Save and continue"}
          </button>
        </div>
      </div>
    </div>
  );
}
