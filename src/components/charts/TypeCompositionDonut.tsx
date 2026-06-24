import type React from "react";
import { formatFileSize } from "@/utils/formatters";
import type { CompSlice } from "./TypeCompositionBar";

interface TypeCompositionDonutProps {
  slices: CompSlice[];
}

const RADIUS = 60;
const STROKE = 30;
const SIZE = (RADIUS + Math.ceil(STROKE / 2)) * 2 + 4; // 138 — derived so stroke never clips
const CIRC = 2 * Math.PI * RADIUS;
const CENTER = SIZE / 2;

/** Donut chart + legend of a folder's file-type composition. */
export const TypeCompositionDonut: React.FC<TypeCompositionDonutProps> = ({ slices }) => {
  if (slices.length === 0) {
    return <div className="text-xs text-muted-foreground">No file-type data</div>;
  }

  const total = slices.reduce((acc, s) => acc + s.size, 0);
  let offset = 0;
  return (
    <div className="flex items-center gap-4">
      <svg width={SIZE} height={SIZE} viewBox={`0 0 ${SIZE} ${SIZE}`} className="shrink-0">
        <circle
          cx={CENTER}
          cy={CENTER}
          r={RADIUS}
          fill="none"
          stroke="var(--border)"
          strokeWidth={STROKE}
          opacity={0.3}
        />
        {slices.map((s, i) => {
          const dash = s.pct * CIRC;
          const seg = (
            <circle
              key={`${s.label}-${i}`}
              cx={CENTER}
              cy={CENTER}
              r={RADIUS}
              fill="none"
              stroke={s.color}
              strokeWidth={STROKE}
              strokeDasharray={`${dash} ${CIRC - dash}`}
              strokeDashoffset={-offset}
              transform={`rotate(-90 ${CENTER} ${CENTER})`}
            />
          );
          offset += dash;
          return seg;
        })}
        <text
          x={CENTER}
          y={CENTER}
          textAnchor="middle"
          dominantBaseline="middle"
          style={{
            fontSize: "10px",
            fontFamily: "var(--font-mono)",
            fill: "var(--foreground)",
            fontVariantNumeric: "tabular-nums",
          }}
        >
          {formatFileSize(total)}
        </text>
      </svg>
      <ul className="space-y-1 text-xs">
        {slices.map((s, i) => (
          <li key={`${s.label}-${i}`} className="flex items-center gap-2 tabular-nums">
            <span
              className="inline-block h-2.5 w-2.5 shrink-0 rounded-sm"
              style={{ background: s.color }}
              aria-hidden
            />
            <span className="font-medium text-foreground">{s.label}</span>
            <span className="text-muted-foreground">
              {Math.round(s.pct * 100)}% · {formatFileSize(s.size)}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
};
