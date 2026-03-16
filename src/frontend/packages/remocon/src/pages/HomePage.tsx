import {
  useListSessionsQuery,
  useCreateSessionMutation,
  useDeleteSessionMutation,
} from "@kf2/common/store";

export function HomePage() {
  const { data, isLoading, error } = useListSessionsQuery();
  const [createSession] = useCreateSessionMutation();
  const [deleteSession] = useDeleteSessionMutation();

  if (isLoading) return <div>Loading sessions…</div>;
  if (error) return <div>Error loading sessions</div>;

  const sessions = data?.sessions ?? [];

  return (
    <div>
      <h1>KF2 Remocon</h1>

      <button type="button" onClick={() => createSession()}>
        New Session
      </button>

      {sessions.length === 0 ? (
        <p>No active sessions</p>
      ) : (
        <ul>
          {sessions.map((s) => (
            <li key={s.id}>
              {s.id}{" "}
              <button type="button" onClick={() => deleteSession({ id: s.id })}>
                Delete
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
