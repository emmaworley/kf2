import type { CSSProperties } from "react";

type BadgeTone = "neutral" | "success" | "warning";

const badgeStyles: Record<BadgeTone, CSSProperties> = {
  neutral: {
    borderColor: "#ccd2db",
    backgroundColor: "#f7f8fa",
    color: "#334155",
  },
  success: {
    borderColor: "#bbe4c4",
    backgroundColor: "#f1fbf4",
    color: "#0f4d1f",
  },
  warning: {
    borderColor: "#f0d9a4",
    backgroundColor: "#fffaef",
    color: "#634700",
  },
};

interface BadgeProps {
  text: string;
  tone?: BadgeTone;
}

export function Badge({ text, tone = "neutral" }: BadgeProps) {
  return (
    <span
      style={{
        display: "inline-block",
        borderWidth: 1,
        borderStyle: "solid",
        borderRadius: 999,
        padding: "2px 8px",
        fontSize: 12,
        lineHeight: "18px",
        fontWeight: 600,
        ...badgeStyles[tone],
      }}
    >
      {text}
    </span>
  );
}
