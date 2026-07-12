import { useEffect, useMemo, useState } from "react";
import { skipToken } from "@reduxjs/toolkit/query";
import {
  useConfigureProviderMutation,
  useCreateSessionMutation,
  useDeleteSessionMutation,
  useListConfiguredProvidersQuery,
  useListProvidersQuery,
  useListSessionsQuery,
  useUnconfigureProviderMutation,
} from "@kf2/common/store";
import {
  Badge,
  Card,
  EmptyState,
  ErrorState,
  LoadingState,
  StateNotice,
} from "@kf2/common/components";
import { getGrpcErrorMessage } from "@kf2/common/api";
import { CapabilityType } from "@kf2/proto/gen/provider_pb.js";
import { ProviderType } from "@kf2/proto/gen/common_pb.js";

type ProviderCredentials = { username: string; password: string };

const pageStyle = {
  maxWidth: 960,
  margin: "0 auto",
  padding: "24px 16px 36px 16px",
  display: "grid",
  gap: 16,
  fontFamily: "Inter, Segoe UI, Roboto, sans-serif",
  color: "#0f172a",
};

const rowStyle = {
  display: "flex",
  gap: 8,
  alignItems: "center",
  flexWrap: "wrap" as const,
};

const buttonBaseStyle = {
  border: "1px solid #c5ccd8",
  borderRadius: 8,
  padding: "6px 10px",
  fontSize: 14,
  lineHeight: "20px",
  cursor: "pointer",
  backgroundColor: "#ffffff",
};

const primaryButtonStyle = {
  ...buttonBaseStyle,
  backgroundColor: "#204ad8",
  borderColor: "#204ad8",
  color: "#ffffff",
};

const destructiveButtonStyle = {
  ...buttonBaseStyle,
  backgroundColor: "#c93737",
  borderColor: "#c93737",
  color: "#ffffff",
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

function capabilityName(capability: CapabilityType): string {
  switch (capability) {
    case CapabilityType.CAPABILITY_SEARCH:
      return "Search";
    case CapabilityType.CAPABILITY_LYRICS:
      return "Lyrics";
    case CapabilityType.CAPABILITY_SCORING:
      return "Scoring";
    default:
      return "Unknown";
  }
}

export function HomePage() {
  const [selectedSessionId, setSelectedSessionId] = useState("");
  const [credentialsByProvider, setCredentialsByProvider] = useState<
    Record<number, ProviderCredentials>
  >({});
  const [actionError, setActionError] = useState<string | null>(null);

  const {
    data: sessionsData,
    isLoading: isSessionsLoading,
    error: sessionsError,
    refetch: refetchSessions,
  } = useListSessionsQuery();
  const [createSession, { isLoading: isCreatingSession }] =
    useCreateSessionMutation();
  const [deleteSession, { isLoading: isDeletingSession }] =
    useDeleteSessionMutation();

  const {
    data: providersData,
    isLoading: isProvidersLoading,
    error: providersError,
    refetch: refetchProviders,
  } = useListProvidersQuery();

  const configuredProvidersArg = selectedSessionId
    ? { sessionId: selectedSessionId }
    : skipToken;
  const {
    data: configuredProvidersData,
    isLoading: isConfiguredProvidersLoading,
    error: configuredProvidersError,
    refetch: refetchConfiguredProviders,
  } = useListConfiguredProvidersQuery(configuredProvidersArg);

  const [configureProvider, { isLoading: isConfiguringProvider }] =
    useConfigureProviderMutation();
  const [unconfigureProvider, { isLoading: isUnconfiguringProvider }] =
    useUnconfigureProviderMutation();

  const sessions = sessionsData?.sessions ?? [];
  const providers = providersData?.providers ?? [];
  const configuredProviders = configuredProvidersData?.providers ?? [];

  useEffect(() => {
    if (selectedSessionId && sessions.some((s) => s.id === selectedSessionId)) {
      return;
    }
    setSelectedSessionId(sessions[0]?.id ?? "");
  }, [sessions, selectedSessionId]);

  const configuredByProvider = useMemo(() => {
    const map = new Map<ProviderType, (typeof configuredProviders)[number]>();
    for (const status of configuredProviders) {
      map.set(status.provider, status);
    }
    return map;
  }, [configuredProviders]);

  const projectorUrl = selectedSessionId
    ? `/projector/#/?session=${encodeURIComponent(selectedSessionId)}`
    : "/projector/";

  function updateCredentials(
    provider: ProviderType,
    key: keyof ProviderCredentials,
    value: string,
  ) {
    setCredentialsByProvider((current) => ({
      ...current,
      [provider]: {
        username: current[provider]?.username ?? "",
        password: current[provider]?.password ?? "",
        [key]: value,
      },
    }));
  }

  async function onCreateSession() {
    setActionError(null);
    try {
      const response = await createSession().unwrap();
      if (response.session?.id) {
        setSelectedSessionId(response.session.id);
      }
    } catch (error) {
      setActionError(getGrpcErrorMessage(error, "Failed to create session"));
    }
  }

  async function onDeleteSession(id: string) {
    setActionError(null);
    try {
      await deleteSession({ id }).unwrap();
      if (selectedSessionId === id) {
        setSelectedSessionId("");
      }
    } catch (error) {
      setActionError(getGrpcErrorMessage(error, "Failed to delete session"));
    }
  }

  async function onConfigureProvider(provider: ProviderType) {
    if (!selectedSessionId) {
      return;
    }

    setActionError(null);
    const credentials = credentialsByProvider[provider] ?? {
      username: "",
      password: "",
    };

    try {
      await configureProvider({
        sessionId: selectedSessionId,
        provider,
        username: credentials.username,
        password: credentials.password,
      }).unwrap();
    } catch (error) {
      setActionError(
        getGrpcErrorMessage(error, `Failed to configure ${providerName(provider)}`),
      );
    }
  }

  async function onUnconfigureProvider(provider: ProviderType) {
    if (!selectedSessionId) {
      return;
    }

    setActionError(null);
    try {
      await unconfigureProvider({
        sessionId: selectedSessionId,
        provider,
      }).unwrap();
    } catch (error) {
      setActionError(
        getGrpcErrorMessage(error, `Failed to unconfigure ${providerName(provider)}`),
      );
    }
  }

  return (
    <main style={pageStyle}>
      <header style={{ display: "grid", gap: 6 }}>
        <h1 style={{ margin: 0, fontSize: 30 }}>KF2 Remocon</h1>
        <p style={{ margin: 0, color: "#4d5562" }}>
          Session host controls for provider setup, queue, and playback.
        </p>
      </header>

      {actionError ? (
        <ErrorState
          message={actionError}
          retryAction={
            <button
              type="button"
              style={buttonBaseStyle}
              onClick={() => setActionError(null)}
            >
              Dismiss
            </button>
          }
        />
      ) : null}

      <Card
        title="Sessions"
        subtitle="Create and select the active karaoke session."
        actions={
          <button
            type="button"
            style={primaryButtonStyle}
            onClick={onCreateSession}
            disabled={isCreatingSession}
          >
            {isCreatingSession ? "Creating..." : "New Session"}
          </button>
        }
      >
        {isSessionsLoading ? (
          <LoadingState label="Loading sessions..." />
        ) : sessionsError ? (
          <ErrorState
            message={getGrpcErrorMessage(sessionsError, "Failed to load sessions")}
            retryAction={
              <button type="button" style={buttonBaseStyle} onClick={refetchSessions}>
                Retry
              </button>
            }
          />
        ) : sessions.length === 0 ? (
          <EmptyState
            title="No sessions yet"
            description="Create a session to start provider setup and playback control."
          />
        ) : (
          <div style={{ display: "grid", gap: 8 }}>
            {sessions.map((session) => {
              const isSelected = session.id === selectedSessionId;
              return (
                <div
                  key={session.id}
                  style={{
                    border: "1px solid #d7dbe3",
                    borderRadius: 8,
                    padding: 10,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 10,
                  }}
                >
                  <div style={{ display: "grid", gap: 2 }}>
                    <button
                      type="button"
                      style={{
                        ...buttonBaseStyle,
                        borderColor: isSelected ? "#204ad8" : "#c5ccd8",
                        color: isSelected ? "#204ad8" : "#0f172a",
                        fontWeight: isSelected ? 700 : 500,
                        textAlign: "left",
                        width: "fit-content",
                      }}
                      onClick={() => setSelectedSessionId(session.id)}
                    >
                      {session.id}
                    </button>
                    <small style={{ color: "#4d5562" }}>
                      Updated {new Date(session.updatedAt).toLocaleString()}
                    </small>
                  </div>
                  <button
                    type="button"
                    style={destructiveButtonStyle}
                    onClick={() => onDeleteSession(session.id)}
                    disabled={isDeletingSession}
                  >
                    Delete
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </Card>

      <Card
        title="Provider configuration"
        subtitle={
          selectedSessionId
            ? `Selected session: ${selectedSessionId}`
            : "Choose a session first"
        }
        actions={
          <a href={projectorUrl} target="_blank" rel="noreferrer">
            Open projector view
          </a>
        }
      >
        {!selectedSessionId ? (
          <EmptyState
            title="No active session selected"
            description="Select or create a session to manage provider auth."
          />
        ) : isProvidersLoading || isConfiguredProvidersLoading ? (
          <LoadingState label="Loading provider state..." />
        ) : providersError || configuredProvidersError ? (
          <ErrorState
            message={getGrpcErrorMessage(
              providersError ?? configuredProvidersError,
              "Failed to load provider information",
            )}
            retryAction={
              <div style={rowStyle}>
                <button type="button" style={buttonBaseStyle} onClick={refetchProviders}>
                  Retry catalog
                </button>
                <button
                  type="button"
                  style={buttonBaseStyle}
                  onClick={refetchConfiguredProviders}
                >
                  Retry status
                </button>
              </div>
            }
          />
        ) : providers.length === 0 ? (
          <EmptyState
            title="No providers registered"
            description="Backend has no enabled providers for this server instance."
          />
        ) : (
          <div style={{ display: "grid", gap: 12 }}>
            {providers.map((provider) => {
              const currentStatus = configuredByProvider.get(provider.provider);
              const isConfigured = currentStatus?.isConfigured ?? false;
              const credentials = credentialsByProvider[provider.provider] ?? {
                username: "",
                password: "",
              };
              const requiresCredentials =
                provider.provider !== ProviderType.PROVIDER_YOUTUBE;
              const isMutating = isConfiguringProvider || isUnconfiguringProvider;

              return (
                <article
                  key={provider.provider}
                  style={{
                    border: "1px solid #d7dbe3",
                    borderRadius: 8,
                    padding: 12,
                    display: "grid",
                    gap: 10,
                  }}
                >
                  <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                    <h3 style={{ margin: 0, fontSize: 18 }}>{provider.name}</h3>
                    <Badge text={providerName(provider.provider)} />
                    <Badge
                      text={isConfigured ? "Configured" : "Not configured"}
                      tone={isConfigured ? "success" : "warning"}
                    />
                  </div>

                  <div style={rowStyle}>
                    {provider.capabilities.map((capability) => (
                      <Badge
                        key={`${provider.provider}-${capability}`}
                        text={capabilityName(capability)}
                      />
                    ))}
                  </div>

                  {isConfigured ? (
                    <div style={rowStyle}>
                      <span style={{ color: "#4d5562", fontSize: 14 }}>
                        {currentStatus?.username
                          ? `Configured as ${currentStatus.username}`
                          : "Configured with provider defaults"}
                      </span>
                      <button
                        type="button"
                        style={buttonBaseStyle}
                        onClick={() => onUnconfigureProvider(provider.provider)}
                        disabled={isMutating}
                      >
                        Unconfigure
                      </button>
                    </div>
                  ) : (
                    <div style={{ display: "grid", gap: 8 }}>
                      {requiresCredentials ? (
                        <div style={rowStyle}>
                          <input
                            style={{
                              flex: "1 1 200px",
                              border: "1px solid #c5ccd8",
                              borderRadius: 8,
                              padding: "7px 10px",
                            }}
                            placeholder="Username"
                            value={credentials.username}
                            onChange={(event) =>
                              updateCredentials(
                                provider.provider,
                                "username",
                                event.target.value,
                              )
                            }
                          />
                          <input
                            style={{
                              flex: "1 1 200px",
                              border: "1px solid #c5ccd8",
                              borderRadius: 8,
                              padding: "7px 10px",
                            }}
                            type="password"
                            placeholder="Password"
                            value={credentials.password}
                            onChange={(event) =>
                              updateCredentials(
                                provider.provider,
                                "password",
                                event.target.value,
                              )
                            }
                          />
                        </div>
                      ) : (
                        <p style={{ margin: 0, color: "#4d5562" }}>
                          This provider does not require credentials.
                        </p>
                      )}

                      <div style={rowStyle}>
                        <button
                          type="button"
                          style={primaryButtonStyle}
                          onClick={() => onConfigureProvider(provider.provider)}
                          disabled={isMutating}
                        >
                          Configure
                        </button>
                      </div>
                    </div>
                  )}
                </article>
              );
            })}
          </div>
        )}
      </Card>

      <Card
        title="Queue and playback"
        subtitle="Frontend baseline placeholder while queue/playback RPCs are finalized."
      >
        <div style={{ display: "grid", gap: 10 }}>
          <StateNotice
            title="Queue management contract pending"
            description="Add/remove/reorder controls will be wired once queue RPCs land in SessionService."
            tone="warning"
          />
          <StateNotice
            title="Playback controls contract pending"
            description="Play/pause/skip/seek controls will be connected once playback endpoints are available."
            tone="info"
          />
        </div>
      </Card>
    </main>
  );
}
