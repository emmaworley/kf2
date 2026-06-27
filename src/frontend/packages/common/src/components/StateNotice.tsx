import type { CSSProperties, ReactNode } from "react";

type NoticeTone = "info" | "success" | "warning" | "error";

const toneStyles: Record<NoticeTone, CSSProperties> = {
  info: {
    borderColor: "#c3d8ff",
    backgroundColor: "#f5f9ff",
    color: "#0f2f5f",
  },
  success: {
    borderColor: "#b7e5c0",
    backgroundColor: "#f2fbf4",
    color: "#123c1d",
  },
  warning: {
    borderColor: "#f0d89b",
    backgroundColor: "#fff9ec",
    color: "#5c4300",
  },
  error: {
    borderColor: "#efb7b7",
    backgroundColor: "#fff4f4",
    color: "#5a1414",
  },
};

const frameStyle: CSSProperties = {
  borderWidth: 1,
  borderStyle: "solid",
  borderRadius: 8,
  padding: 12,
};

const titleStyle: CSSProperties = {
  margin: 0,
  fontSize: 15,
  lineHeight: "22px",
  fontWeight: 600,
};

const descriptionStyle: CSSProperties = {
  margin: "4px 0 0 0",
  fontSize: 14,
  lineHeight: "20px",
};

const actionStyle: CSSProperties = {
  marginTop: 8,
};

interface StateNoticeProps {
  title: string;
  description?: string;
  tone?: NoticeTone;
  action?: ReactNode;
}

export function StateNotice({
  title,
  description,
  tone = "info",
  action,
}: StateNoticeProps) {
  return (
    <div style={{ ...frameStyle, ...toneStyles[tone] }}>
      <p style={titleStyle}>{title}</p>
      {description ? <p style={descriptionStyle}>{description}</p> : null}
      {action ? <div style={actionStyle}>{action}</div> : null}
    </div>
  );
}

interface LoadingStateProps {
  label?: string;
}

export function LoadingState({ label = "Loading…" }: LoadingStateProps) {
  return <StateNotice title={label} tone="info" />;
}

interface ErrorStateProps {
  message: string;
  retryAction?: ReactNode;
}

export function ErrorState({ message, retryAction }: ErrorStateProps) {
  return <StateNotice title="Something went wrong" description={message} tone="error" action={retryAction} />;
}

interface EmptyStateProps {
  title: string;
  description?: string;
  action?: ReactNode;
}

export function EmptyState({ title, description, action }: EmptyStateProps) {
  return <StateNotice title={title} description={description} tone="warning" action={action} />;
}
