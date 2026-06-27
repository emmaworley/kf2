import type { CSSProperties, ReactNode } from "react";

const shellStyle: CSSProperties = {
  border: "1px solid #d4d8de",
  borderRadius: 10,
  padding: 16,
  backgroundColor: "#ffffff",
};

const titleRowStyle: CSSProperties = {
  display: "flex",
  alignItems: "flex-start",
  justifyContent: "space-between",
  gap: 12,
  marginBottom: 12,
};

const titleStyle: CSSProperties = {
  margin: 0,
  fontSize: 18,
  lineHeight: "24px",
};

const subtitleStyle: CSSProperties = {
  margin: "4px 0 0 0",
  color: "#4d5562",
  fontSize: 14,
  lineHeight: "20px",
};

interface CardProps {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
  children: ReactNode;
}

export function Card({ title, subtitle, actions, children }: CardProps) {
  return (
    <section style={shellStyle}>
      <header style={titleRowStyle}>
        <div>
          <h2 style={titleStyle}>{title}</h2>
          {subtitle ? <p style={subtitleStyle}>{subtitle}</p> : null}
        </div>
        {actions}
      </header>
      {children}
    </section>
  );
}
