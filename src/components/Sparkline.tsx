interface SparklineProps {
  values: number[];
  className?: string;
}

const WIDTH = 100;
const HEIGHT = 28;

function buildLine(values: number[]) {
  if (values.length === 0) {
    return `M0,${HEIGHT - 1} L${WIDTH},${HEIGHT - 1}`;
  }

  if (values.length === 1) {
    const max = Math.max(values[0], 1) * 1.15;
    const y = HEIGHT - 1 - (values[0] / max) * (HEIGHT - 3);
    return `M0,${y} L${WIDTH},${y}`;
  }

  const max = Math.max(...values, 1) * 1.15;
  const points = values.map((value, index) => {
    const x =
      values.length === 1 ? WIDTH : (index / (values.length - 1)) * WIDTH;
    const y = HEIGHT - 1 - (value / max) * (HEIGHT - 3);
    return [x, y] as const;
  });

  return points
    .map(([x, y], index) => `${index === 0 ? "M" : "L"}${x},${y}`)
    .join(" ");
}

export function Sparkline({ values, className = "" }: SparklineProps) {
  return (
    <svg
      className={["sparkline", className].filter(Boolean).join(" ")}
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      preserveAspectRatio="none"
      aria-hidden="true"
      focusable="false"
    >
      <path className="sparkline__line" d={buildLine(values)} />
    </svg>
  );
}
