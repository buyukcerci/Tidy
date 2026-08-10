import { useCallback, useEffect, useRef, useState } from "react";
import {
  beginGoogleConnect,
  cancelGoogleConnect,
  disconnectGoogleAccount,
  getConnectionStatus,
  type ConnectionStatus,
  type DisconnectOutcome,
} from "../lib/oauth";

export function useConnection() {
  const [status, setStatus] = useState<ConnectionStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [connectError, setConnectError] = useState<string | null>(null);
  const [disconnecting, setDisconnecting] = useState(false);
  const cancelledRef = useRef(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await getConnectionStatus());
      setStatusError(null);
    } catch (error) {
      setStatusError(String(error));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const connect = useCallback(async () => {
    cancelledRef.current = false;
    setConnectError(null);
    setConnecting(true);
    try {
      await beginGoogleConnect();
    } catch (error) {
      if (!cancelledRef.current) {
        setConnectError(String(error));
      }
    } finally {
      setConnecting(false);
      await refresh();
    }
  }, [refresh]);

  const cancelConnect = useCallback(() => {
    cancelledRef.current = true;
    void cancelGoogleConnect();
  }, []);

  const disconnect = useCallback(
    async (eraseLocalData: boolean): Promise<DisconnectOutcome | null> => {
      setDisconnecting(true);
      try {
        const outcome = await disconnectGoogleAccount(eraseLocalData);
        await refresh();
        return outcome;
      } finally {
        setDisconnecting(false);
      }
    },
    [refresh]
  );

  return {
    status,
    statusError,
    connecting,
    connectError,
    disconnecting,
    refresh,
    connect,
    cancelConnect,
    disconnect,
  };
}