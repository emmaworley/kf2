import { useEffect, useMemo, useState } from "react";
import { skipToken } from "@reduxjs/toolkit/query";
import {
  useListConfiguredProvidersQuery,
  useListSessionsQuery,
} from "@kf2/common/store";
import {
  Card,
  EmptyState,
  ErrorState,
  LoadingState,
  StateNotice,
} from "@kf2/common/components";
import { getGrpcErrorMessage } from "@kf2/common/api";
import { ProviderType } from "@kf2/proto/gen/common_pb.js";

const pageStyle = {
  maxWidth: 1100,
  margin: "0 auto",
  padding: "22px 18px 34px 18px",
  display: "grid",
  gap: 16,
  color: "#e2e8f0",
  fontFamily: "Inter, Segoe UI, Roboto, sans-serif",
};

const shellStyle = {
  minHeight: "100vh",
  backgroundColor: "#020617",
};

const labelStyle = {
  color: "#94a3b8",
  fontSize: 14,
};

function providerName(provider: ProviderType): string {
  switch (provider) {
    case ProviderType.PROVIDER_DAM:
      return "DAM";
    case ProviderType.PROVIDER_JOYSOUND:
      return "Joysound";
    case ProviderType.PROVIDER_YOUTUBE:
      return "YouTube";
    default:
      return "Unknown provider";
  }
}

function readHashSessionId(): string {
  const hash = window.location.hash ?? "";
  const queryIndex = hash.indexOf("?");
  if (queryIndex < 0) {
    return "";
  }
  const params = new URLSearchParams(hash.slice(queryIndex + 1));
  return params.get("session") ?? "";
}

export function App() {
  const [selectedSessionId, setSelectedSessionId] = useState(readHashSessionId);

  const {
    data: sessionsData,
    isLoading: isSessionsLoading,
    error: sessionsError,
    refetch: refetchSessions,
  } = useListSessionsQuery();

  const sessions = sessionsData?.sessions ?? [];

  useEffect(() => {
    if (selectedSessionId && sessions.some((session) => session.id === selectedSessionId)) {
      return;
    }
    setSelectedSessionId(sessions[0]?.id ?? "");
  }, [sessions, selectedSessionId]);

  useEffect(() => {
    const targetHash = selectedSessionId
      ? `/?session=${encodeURIComponent(selectedSessionId)}`
      : "/";
    if (window.location.hash !== `#${targetHash}`) {
      window.history.replaceState(null, "", `${window.location.pathname}#${targetHash}`);
    }
  }, [selectedSessionId]);

  const configuredProvidersArg = selectedSessionId
    ? { sessionId: selectedSessionId }
    : skipToken;
  const {
    data: configuredProvidersData,
    isLoading: isConfiguredProvidersLoading,
    error: configuredProvidersError,
    refetch: refetchConfiguredProviders,
  } = useListConfiguredProvidersQuery(configuredProvidersArg);

  const configuredProviders = configuredProvidersData?.providers ?? [];
  const connectedCount = configuredProviders.filter((provider) => provider.isConfigured).length;

  const currentSession = useMemo(
    () => sessions.find((session) => session.id === selectedSessionId),
    [sessions, selectedSessionId],
  );

  return (
    <div style={shellStyle}>
      <main style={pageStyle}>
        <header style={{ display: "grid", gap: 6 }}>
          <h1 style={{ margin: 0, fontSize: 38 }}>KF2 Projector</h1>
          <p style={{ margin: 0, color: "#94a3b8" }}>
            Live karaoke display baseline for session playback and queue projection.
          </p>
        </header>

        <Card
          title="Session binding"
          subtitle="Choose which session this projector should reflect."
          actions={
            <button
              type="button"
              style={{
                border: "1px solid #475569",
                borderRadius: 8,
                backgroundColor: "#0f172a",
                color: "#e2e8f0",
                padding: "6px 10px",
                cursor: "pointer",
              }}
              onClick={refetchSessions}
            >
              Refresh
            </button>
          }
        >
          {isSessionsLoading ? (
            <LoadingState label="Connecting to session service..." />
          ) : sessionsError ? (
            <ErrorState
              message={getGrpcErrorMessage(sessionsError, "Failed to load sessions")}
              retryAction={
                <button type="button" onClick={refetchSessions}>
                  Retry
                </button>
              }
            />
          ) : sessions.length === 0 ? (
            <EmptyState
              title="No active sessions"
              description="Create a session from remocon to start a projector view."
            />
          ) : (
            <div style={{ display: "grid", gap: 8 }}>
              {sessions.map((session) => {
                const isSelected = session.id === selectedSessionId;
                return (
                  <label
                    key={session.id}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: 10,
                      border: "1px solid #334155",
                      borderRadius: 8,
                      padding: 10,
                      backgroundColor: isSelected ? "#11203f" : "#0b1224",
                    }}
                  >
                    <span>
                      <input
                        type="radio"
                        checked={isSelected}
                        onChange={() => setSelectedSessionId(session.id)}
                      />{" "}
                      {session.id}
                    </span>
                    <small style={labelStyle}>
                      Updated {new Date(session.updatedAt).toLocaleString()}
                    </small>
                  </label>
                );
              })}
            </div>
          )}
        </Card>

        <Card
          title="Connection status"
          subtitle={
            currentSession
              ? `Connected to session ${currentSession.id}`
              : "No session selected"
          }
        >
          {!selectedSessionId ? (
            <EmptyState
              title="Waiting for session selection"
              description="Select a session to show provider readiness and playback placeholders."
            />
          ) : isConfiguredProvidersLoading ? (
            <LoadingState label="Loading provider readiness..." />
          ) : configuredProvidersError ? (
            <ErrorState
              message={getGrpcErrorMessage(
                configuredProvidersError,
                "Failed to load provider status",
              )}
              retryAction={
                <button type="button" onClick={refetchConfiguredProviders}>
                  Retry
                </button>
              }
            />
          ) : (
            <div style={{ display: "grid", gap: 10 }}>
              <p style={{ margin: 0 }}>
                Configured providers: <strong>{connectedCount}</strong>
              </p>
              {configuredProviders.length === 0 ? (
                <StateNotice
                  title="No providers configured for this session"
                  description="Use remocon to authenticate at least one provider."
                  tone="warning"
                />
              ) : (
                <ul style={{ margin: 0, paddingLeft: 18 }}>
                  {configuredProviders.map((status) => (
                    <li key={`${status.provider}-${status.username ?? "default"}`}>
                      {providerName(status.provider)} -{" "}
                      {status.username ? `as ${status.username}` : "configured"}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </Card>

        <Card title="Now playing" subtitle="Playback rendering baseline">
          <StateNotice
            title="Playback stream wiring pending"
            description="This area will render video, lyrics, and timing once playback/queue RPCs are available."
            tone="info"
          />
        </Card>

        <Card title="Upcoming queue" subtitle="Projected upcoming songs">
          <StateNotice
            title="Queue projection contract pending"
            description="Queue display will be connected after queue CRUD and playback-state services are added."
            tone="warning"
          />
        </Card>
      </main>
    </div>
  );
}
