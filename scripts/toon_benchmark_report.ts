import type { ToonExportBenchmarkReport } from "./benchmark_toon_exports";

type DatasetResult = ToonExportBenchmarkReport["datasets"][number];
type VariantMeasurement = DatasetResult["measurements"][keyof DatasetResult["measurements"]];

export type BenchmarkChartArtifact = {
  key: string;
  title: string;
  filename: string;
  svg: string;
};

const DATASET_LABELS: Record<DatasetResult["id"], string> = {
  uniform_tick_event_batch: "Events",
  toonscape_donor_tick_event_batch: "Toonscape Donor",
  semi_uniform_agent_logs: "Logs",
  nested_world_snapshot: "World",
  deep_multiverse_index: "Multiverse",
};

const DATASET_TITLES: Record<DatasetResult["id"], string> = {
  uniform_tick_event_batch: "Uniform Tick Event Batch",
  toonscape_donor_tick_event_batch: "Toonscape Donor Tick Event Batch",
  semi_uniform_agent_logs: "Semi-Uniform Agent Logs",
  nested_world_snapshot: "Nested World Snapshot",
  deep_multiverse_index: "Deep Multiverse Index",
};

const VARIANT_COLORS: Record<string, string> = {
  "json-pretty": "#d1495b",
  "json-compact": "#edae49",
  "toon-comma": "#00798c",
  "toon-tab": "#30638e",
  encode: "#2a9d8f",
  decode: "#e76f51",
  delta: "#4f5d75",
};

const PAGE_TITLE = "TOON Export Benchmark Results";

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function formatInteger(value: number): string {
  return value.toLocaleString("en-US", {
    maximumFractionDigits: 0,
  });
}

function formatPercent(value: number): string {
  return `${value >= 0 ? "+" : ""}${value.toFixed(1)}%`;
}

function formatBytes(value: number): string {
  return `${formatInteger(value)} B`;
}

function formatTokens(value: number): string {
  return `${formatInteger(value)} tok`;
}

function formatNs(value: number): string {
  if (value >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(2)} ms`;
  }
  if (value >= 1_000) {
    return `${(value / 1_000).toFixed(1)} us`;
  }
  return `${value.toFixed(0)} ns`;
}

function formatDate(unixMs: number): string {
  return new Date(unixMs).toLocaleString("en-US", {
    dateStyle: "medium",
    timeStyle: "medium",
  });
}

function fileStem(value: string): string {
  return value
    .replaceAll(/[^a-z0-9]+/gi, "-")
    .replaceAll(/^-+|-+$/g, "")
    .toLowerCase();
}

function variantLabel(measurement: VariantMeasurement): string {
  return measurement.label.replace("TOON ", "").replace(/[()]/g, "");
}

function buildLegend(
  items: Array<{ label: string; color: string }>,
  width: number,
): string {
  const itemWidth = Math.max(140, Math.floor(width / Math.max(1, items.length)));
  return items
    .map((item, index) => {
      const x = 24 + index * itemWidth;
      return `
        <g transform="translate(${x} 16)">
          <rect x="0" y="0" width="14" height="14" rx="3" fill="${item.color}" />
          <text x="22" y="11" font-size="12" fill="#1f2937">${escapeHtml(item.label)}</text>
        </g>
      `;
    })
    .join("");
}

function renderGroupedBarChartSvg(options: {
  title: string;
  categories: string[];
  series: Array<{
    id: string;
    label: string;
    color: string;
    values: number[];
  }>;
  formatter: (value: number) => string;
  valueSuffix?: string;
  width?: number;
  height?: number;
}): string {
  const width = options.width ?? 880;
  const height = options.height ?? 360;
  const margin = {
    top: 54,
    right: 24,
    bottom: 76,
    left: 76,
  };
  const plotWidth = width - margin.left - margin.right;
  const plotHeight = height - margin.top - margin.bottom;
  const values = options.series.flatMap((series) => series.values);
  const minValue = Math.min(0, ...values);
  const maxValue = Math.max(0, ...values, 1);
  const range = maxValue - minValue || 1;
  const ticks = 5;
  const yScale = (value: number) =>
    margin.top + ((maxValue - value) / range) * plotHeight;
  const zeroY = yScale(0);
  const groupWidth = plotWidth / Math.max(1, options.categories.length);
  const slotWidth = groupWidth / Math.max(1, options.series.length);
  const barWidth = Math.max(14, Math.min(36, slotWidth - 12));

  const grid = Array.from({ length: ticks + 1 }, (_, index) => {
    const value = minValue + (range * index) / ticks;
    const y = yScale(value);
    return `
      <g>
        <line x1="${margin.left}" y1="${y}" x2="${width - margin.right}" y2="${y}" stroke="#d9e2ec" stroke-width="1" />
        <text x="${margin.left - 12}" y="${y + 4}" text-anchor="end" font-size="11" fill="#52606d">${escapeHtml(options.formatter(value))}</text>
      </g>
    `;
  }).join("");

  const bars = options.categories
    .map((category, categoryIndex) => {
      const categoryCenter = margin.left + groupWidth * categoryIndex + groupWidth / 2;
      const xLabel = `
        <text x="${categoryCenter}" y="${height - 24}" text-anchor="middle" font-size="12" fill="#1f2937">${escapeHtml(category)}</text>
      `;

      const seriesRects = options.series
        .map((series, seriesIndex) => {
          const value = series.values[categoryIndex] ?? 0;
          const barX =
            margin.left +
            categoryIndex * groupWidth +
            seriesIndex * slotWidth +
            (slotWidth - barWidth) / 2;
          const valueY = yScale(value);
          const barY = Math.min(zeroY, valueY);
          const barHeight = Math.max(2, Math.abs(zeroY - valueY));
          const labelY = value >= 0 ? barY - 8 : barY + barHeight + 14;
          return `
            <g>
              <rect x="${barX}" y="${barY}" width="${barWidth}" height="${barHeight}" rx="6" fill="${series.color}" opacity="0.92" />
              <text x="${barX + barWidth / 2}" y="${labelY}" text-anchor="middle" font-size="11" fill="#102a43">${escapeHtml(options.formatter(value))}</text>
            </g>
          `;
        })
        .join("");

      return `${seriesRects}${xLabel}`;
    })
    .join("");

  return `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="${fileStem(options.title)}-title">
  <title id="${fileStem(options.title)}-title">${escapeHtml(options.title)}</title>
  <rect width="${width}" height="${height}" rx="18" fill="#f8fafc" />
  <text x="24" y="30" font-size="20" font-weight="700" fill="#102a43">${escapeHtml(options.title)}</text>
  ${buildLegend(
    options.series.map((series) => ({ label: series.label, color: series.color })),
    width - 48,
  )}
  ${grid}
  <line x1="${margin.left}" y1="${zeroY}" x2="${width - margin.right}" y2="${zeroY}" stroke="#7b8794" stroke-width="1.5" />
  ${bars}
</svg>`;
}

function renderScatterPlotSvg(options: {
  title: string;
  points: Array<{
    label: string;
    color: string;
    x: number;
    y: number;
  }>;
  xFormatter: (value: number) => string;
  yFormatter: (value: number) => string;
  width?: number;
  height?: number;
}): string {
  const width = options.width ?? 880;
  const height = options.height ?? 360;
  const margin = {
    top: 54,
    right: 160,
    bottom: 64,
    left: 76,
  };
  const plotWidth = width - margin.left - margin.right;
  const plotHeight = height - margin.top - margin.bottom;
  const maxX = Math.max(1, ...options.points.map((point) => point.x));
  const maxY = Math.max(1, ...options.points.map((point) => point.y));
  const xScale = (value: number) =>
    margin.left + (value / maxX) * plotWidth;
  const yScale = (value: number) =>
    margin.top + plotHeight - (value / maxY) * plotHeight;
  const ticks = 4;

  const verticalGrid = Array.from({ length: ticks + 1 }, (_, index) => {
    const value = (maxX * index) / ticks;
    const x = xScale(value);
    return `
      <g>
        <line x1="${x}" y1="${margin.top}" x2="${x}" y2="${height - margin.bottom}" stroke="#d9e2ec" stroke-width="1" />
        <text x="${x}" y="${height - 28}" text-anchor="middle" font-size="11" fill="#52606d">${escapeHtml(options.xFormatter(value))}</text>
      </g>
    `;
  }).join("");

  const horizontalGrid = Array.from({ length: ticks + 1 }, (_, index) => {
    const value = (maxY * index) / ticks;
    const y = yScale(value);
    return `
      <g>
        <line x1="${margin.left}" y1="${y}" x2="${width - margin.right}" y2="${y}" stroke="#d9e2ec" stroke-width="1" />
        <text x="${margin.left - 12}" y="${y + 4}" text-anchor="end" font-size="11" fill="#52606d">${escapeHtml(options.yFormatter(value))}</text>
      </g>
    `;
  }).join("");

  const points = options.points
    .map((point) => {
      const x = xScale(point.x);
      const y = yScale(point.y);
      return `
        <g>
          <circle cx="${x}" cy="${y}" r="8" fill="${point.color}" opacity="0.9" />
          <text x="${x + 12}" y="${y + 4}" font-size="12" fill="#102a43">${escapeHtml(point.label)}</text>
        </g>
      `;
    })
    .join("");

  const legend = options.points
    .map((point, index) => {
      const y = margin.top + 12 + index * 24;
      return `
        <g transform="translate(${width - margin.right + 16} ${y})">
          <circle cx="7" cy="7" r="7" fill="${point.color}" />
          <text x="22" y="11" font-size="12" fill="#1f2937">${escapeHtml(point.label)}</text>
        </g>
      `;
    })
    .join("");

  return `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="${fileStem(options.title)}-title">
  <title id="${fileStem(options.title)}-title">${escapeHtml(options.title)}</title>
  <rect width="${width}" height="${height}" rx="18" fill="#f8fafc" />
  <text x="24" y="30" font-size="20" font-weight="700" fill="#102a43">${escapeHtml(options.title)}</text>
  ${verticalGrid}
  ${horizontalGrid}
  <line x1="${margin.left}" y1="${height - margin.bottom}" x2="${width - margin.right}" y2="${height - margin.bottom}" stroke="#7b8794" stroke-width="1.5" />
  <line x1="${margin.left}" y1="${margin.top}" x2="${margin.left}" y2="${height - margin.bottom}" stroke="#7b8794" stroke-width="1.5" />
  <text x="${margin.left + plotWidth / 2}" y="${height - 8}" text-anchor="middle" font-size="12" fill="#52606d">Bytes</text>
  <text x="18" y="${margin.top + plotHeight / 2}" transform="rotate(-90 18 ${margin.top + plotHeight / 2})" text-anchor="middle" font-size="12" fill="#52606d">Tokens</text>
  ${points}
  ${legend}
</svg>`;
}

function renderVariantTable(dataset: DatasetResult): string {
  return Object.values(dataset.measurements)
    .map((measurement) => {
      const recommendation =
        dataset.recommendation.preferredVariant === measurement.variantId;
      return `
        <tr${recommendation ? ' class="winner-row"' : ""}>
          <td>${escapeHtml(measurement.label)}</td>
          <td>${formatBytes(measurement.bytes)}</td>
          <td>${formatTokens(measurement.tokens)}</td>
          <td>${formatNs(measurement.encode.medianNsPerOperation)}</td>
          <td>${formatNs(measurement.decode.medianNsPerOperation)}</td>
          <td>${measurement.roundtripMatches ? "yes" : "no"}</td>
        </tr>
      `;
    })
    .join("");
}

function buildDatasetCharts(dataset: DatasetResult): BenchmarkChartArtifact[] {
  const measurements = Object.values(dataset.measurements);
  const categories = measurements.map((measurement) => variantLabel(measurement));

  return [
    {
      key: `${dataset.id}-bytes`,
      title: `${DATASET_TITLES[dataset.id]} Bytes`,
      filename: `${dataset.id}-bytes.svg`,
      svg: renderGroupedBarChartSvg({
        title: `${DATASET_TITLES[dataset.id]}: bytes by variant`,
        categories,
        series: [
          {
            id: "bytes",
            label: "Bytes",
            color: VARIANT_COLORS.delta,
            values: measurements.map((measurement) => measurement.bytes),
          },
        ],
        formatter: formatBytes,
      }),
    },
    {
      key: `${dataset.id}-tokens`,
      title: `${DATASET_TITLES[dataset.id]} Tokens`,
      filename: `${dataset.id}-tokens.svg`,
      svg: renderGroupedBarChartSvg({
        title: `${DATASET_TITLES[dataset.id]}: tokens by variant`,
        categories,
        series: [
          {
            id: "tokens",
            label: "Tokens",
            color: "#7c3aed",
            values: measurements.map((measurement) => measurement.tokens),
          },
        ],
        formatter: formatTokens,
      }),
    },
    {
      key: `${dataset.id}-latency`,
      title: `${DATASET_TITLES[dataset.id]} Latency`,
      filename: `${dataset.id}-latency.svg`,
      svg: renderGroupedBarChartSvg({
        title: `${DATASET_TITLES[dataset.id]}: median encode/decode latency`,
        categories,
        series: [
          {
            id: "encode",
            label: "Encode median",
            color: VARIANT_COLORS.encode,
            values: measurements.map(
              (measurement) => measurement.encode.medianNsPerOperation,
            ),
          },
          {
            id: "decode",
            label: "Decode median",
            color: VARIANT_COLORS.decode,
            values: measurements.map(
              (measurement) => measurement.decode.medianNsPerOperation,
            ),
          },
        ],
        formatter: formatNs,
      }),
    },
    {
      key: `${dataset.id}-efficiency`,
      title: `${DATASET_TITLES[dataset.id]} Efficiency`,
      filename: `${dataset.id}-efficiency.svg`,
      svg: renderScatterPlotSvg({
        title: `${DATASET_TITLES[dataset.id]}: bytes vs tokens`,
        points: measurements.map((measurement) => ({
          label: variantLabel(measurement),
          color: VARIANT_COLORS[measurement.variantId],
          x: measurement.bytes,
          y: measurement.tokens,
        })),
        xFormatter: formatBytes,
        yFormatter: formatTokens,
      }),
    },
  ];
}

export function renderToonBenchmarkCharts(
  report: ToonExportBenchmarkReport,
): BenchmarkChartArtifact[] {
  const datasets = report.datasets;
  const overviewCharts: BenchmarkChartArtifact[] = [
    {
      key: "overview-bytes-delta",
      title: "Best TOON Byte Delta",
      filename: "overview-bytes-delta.svg",
      svg: renderGroupedBarChartSvg({
        title: "Best TOON delta vs compact JSON: bytes",
        categories: datasets.map((dataset) => DATASET_LABELS[dataset.id]),
        series: [
          {
            id: "delta",
            label: "Best TOON delta",
            color: VARIANT_COLORS.delta,
            values: datasets.map((dataset) => dataset.bestToonDeltaVsCompactJson.bytes),
          },
        ],
        formatter: formatBytes,
      }),
    },
    {
      key: "overview-token-delta",
      title: "Best TOON Token Delta",
      filename: "overview-token-delta.svg",
      svg: renderGroupedBarChartSvg({
        title: "Best TOON delta vs compact JSON: tokens",
        categories: datasets.map((dataset) => DATASET_LABELS[dataset.id]),
        series: [
          {
            id: "delta",
            label: "Best TOON delta",
            color: "#7c3aed",
            values: datasets.map((dataset) => dataset.bestToonDeltaVsCompactJson.tokens),
          },
        ],
        formatter: formatTokens,
      }),
    },
  ];

  return [...overviewCharts, ...datasets.flatMap((dataset) => buildDatasetCharts(dataset))];
}

export function renderToonBenchmarkMarkdown(
  report: ToonExportBenchmarkReport,
): string {
  const datasetLines = report.datasets
    .map((dataset) => {
      const recommendation = dataset.recommendation;
      const deltaBytes = formatPercent(dataset.bestToonDeltaVsCompactJson.percentBytes);
      const deltaTokens = formatPercent(dataset.bestToonDeltaVsCompactJson.percentTokens);
      return `- ${DATASET_TITLES[dataset.id]}: prefer \`${recommendation.preferredFormat}\` via \`${recommendation.preferredVariant}\`; TOON delta vs compact JSON = ${deltaBytes} bytes, ${deltaTokens} tokens.`;
    })
    .join("\n");

  const checkLines = report.checks
    .map((check) => `- ${check.passed ? "PASS" : "FAIL"} \`${check.metric}\`: observed \`${check.observed}\`, expected \`${check.expected}\``)
    .join("\n");

  return `# ${PAGE_TITLE}

- Generated: ${formatDate(report.generatedAtUnixMs)}
- Iterations: ${formatInteger(report.iterations)}
- Rounds: ${formatInteger(report.rounds)}
- Shell control plane: \`${report.decision.shellControlPlane}\`
- Checks passed: ${report.checks.filter((check) => check.passed).length}/${report.checks.length}

## Recommendations

${datasetLines}

## Validation

- Row-width strict decode: \`${report.validation.strictRowWidthError}\`
- Truncation strict decode: \`${report.validation.strictTruncationError}\`

## Checks

${checkLines}
`;
}

export function renderToonBenchmarkHtml(
  report: ToonExportBenchmarkReport,
): string {
  const charts = renderToonBenchmarkCharts(report);
  const chartMap = new Map(charts.map((chart) => [chart.key, chart]));
  const summaryCards = report.datasets
    .map((dataset) => {
      const recommendation = dataset.recommendation;
      const deltaBytes = dataset.bestToonDeltaVsCompactJson.percentBytes;
      const deltaTokens = dataset.bestToonDeltaVsCompactJson.percentTokens;
      return `
        <article class="summary-card">
          <div class="eyebrow">${escapeHtml(DATASET_LABELS[dataset.id])}</div>
          <h3>${escapeHtml(recommendation.preferredFormat.toUpperCase())}</h3>
          <p>${escapeHtml(recommendation.preferredVariant)}</p>
          <dl>
            <div><dt>Bytes</dt><dd>${formatPercent(deltaBytes)}</dd></div>
            <div><dt>Tokens</dt><dd>${formatPercent(deltaTokens)}</dd></div>
          </dl>
        </article>
      `;
    })
    .join("");

  const datasetSections = report.datasets
    .map((dataset) => {
      const recommendation = dataset.recommendation;
      const notes = recommendation.rationale
        .map((item) => `<li>${escapeHtml(item)}</li>`)
        .join("");
      return `
        <section class="dataset-section">
          <div class="dataset-header">
            <div>
              <div class="eyebrow">${escapeHtml(dataset.family)}</div>
              <h2>${escapeHtml(DATASET_TITLES[dataset.id])}</h2>
              <p>${escapeHtml(dataset.description)}</p>
            </div>
            <div class="recommendation-pill ${escapeHtml(recommendation.preferredFormat)}">
              ${escapeHtml(recommendation.preferredFormat.toUpperCase())}
            </div>
          </div>
          <div class="metrics-grid">
            <article class="metric-card">
              <span>Best byte variant</span>
              <strong>${escapeHtml(dataset.bestByteVariant)}</strong>
            </article>
            <article class="metric-card">
              <span>Best token variant</span>
              <strong>${escapeHtml(dataset.bestTokenVariant)}</strong>
            </article>
            <article class="metric-card">
              <span>Best TOON delta</span>
              <strong>${formatPercent(dataset.bestToonDeltaVsCompactJson.percentBytes)} bytes</strong>
            </article>
            <article class="metric-card">
              <span>Best TOON delta</span>
              <strong>${formatPercent(dataset.bestToonDeltaVsCompactJson.percentTokens)} tokens</strong>
            </article>
          </div>
          <ul class="rationale-list">${notes}</ul>
          <div class="chart-grid">
            <figure class="chart-card">${chartMap.get(`${dataset.id}-bytes`)?.svg ?? ""}</figure>
            <figure class="chart-card">${chartMap.get(`${dataset.id}-tokens`)?.svg ?? ""}</figure>
            <figure class="chart-card">${chartMap.get(`${dataset.id}-latency`)?.svg ?? ""}</figure>
            <figure class="chart-card">${chartMap.get(`${dataset.id}-efficiency`)?.svg ?? ""}</figure>
          </div>
          <div class="table-card">
            <table>
              <thead>
                <tr>
                  <th>Variant</th>
                  <th>Bytes</th>
                  <th>Tokens</th>
                  <th>Encode median</th>
                  <th>Decode median</th>
                  <th>Roundtrip</th>
                </tr>
              </thead>
              <tbody>
                ${renderVariantTable(dataset)}
              </tbody>
            </table>
          </div>
        </section>
      `;
    })
    .join("");

  const checkRows = report.checks
    .map(
      (check) => `
        <tr${check.passed ? "" : ' class="failed-row"'}>
          <td>${escapeHtml(check.metric)}</td>
          <td>${check.passed ? "pass" : "fail"}</td>
          <td>${escapeHtml(check.observed)}</td>
          <td>${escapeHtml(check.expected)}</td>
        </tr>
      `,
    )
    .join("");

  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${PAGE_TITLE}</title>
    <style>
      :root {
        color-scheme: light;
        --bg: #f3f4f6;
        --surface: #ffffff;
        --surface-alt: #f8fafc;
        --ink: #102a43;
        --muted: #52606d;
        --border: #d9e2ec;
        --good: #0f766e;
        --warn: #7c3aed;
        --bad: #b91c1c;
        --shadow: 0 18px 40px rgba(15, 23, 42, 0.08);
      }
      * { box-sizing: border-box; }
      body {
        margin: 0;
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        background: radial-gradient(circle at top, #ffffff 0%, var(--bg) 55%);
        color: var(--ink);
      }
      main {
        width: min(1440px, calc(100vw - 40px));
        margin: 0 auto;
        padding: 32px 0 72px;
      }
      header.hero, section.panel {
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: 24px;
        box-shadow: var(--shadow);
        padding: 28px;
        margin-bottom: 24px;
      }
      .hero-grid, .summary-grid, .metrics-grid, .chart-grid {
        display: grid;
        gap: 16px;
      }
      .hero-grid { grid-template-columns: 2fr 1fr; }
      .summary-grid { grid-template-columns: repeat(4, minmax(0, 1fr)); margin-top: 20px; }
      .metrics-grid { grid-template-columns: repeat(4, minmax(0, 1fr)); }
      .chart-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); margin-top: 18px; }
      h1, h2, h3, p { margin: 0; }
      h1 { font-size: 2.2rem; margin-bottom: 12px; }
      h2 { font-size: 1.55rem; margin-bottom: 8px; }
      h3 { font-size: 1.2rem; }
      .lede { color: var(--muted); font-size: 1rem; line-height: 1.6; }
      .eyebrow {
        font-size: 0.78rem;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        color: var(--warn);
        margin-bottom: 8px;
      }
      .meta-list, .rationale-list {
        margin: 16px 0 0;
        padding-left: 18px;
        color: var(--muted);
      }
      .meta-card, .summary-card, .metric-card, .table-card, .chart-card {
        background: var(--surface-alt);
        border: 1px solid var(--border);
        border-radius: 18px;
        padding: 18px;
      }
      .meta-card strong, .metric-card strong { display: block; margin-top: 6px; font-size: 1.05rem; }
      .summary-card h3 { margin-top: 8px; font-size: 1.9rem; }
      .summary-card p { color: var(--muted); margin: 4px 0 12px; }
      .summary-card dl {
        display: grid;
        gap: 10px;
        margin: 0;
      }
      .summary-card dl div {
        display: flex;
        justify-content: space-between;
      }
      .summary-card dt { color: var(--muted); }
      .summary-card dd { margin: 0; font-weight: 700; }
      .dataset-section + .dataset-section { margin-top: 28px; }
      .dataset-header {
        display: flex;
        justify-content: space-between;
        gap: 20px;
        align-items: flex-start;
        margin-bottom: 18px;
      }
      .dataset-header p { color: var(--muted); line-height: 1.6; }
      .recommendation-pill {
        padding: 10px 14px;
        border-radius: 999px;
        font-size: 0.78rem;
        font-weight: 700;
        letter-spacing: 0.08em;
      }
      .recommendation-pill.toon { background: rgba(48, 99, 142, 0.12); color: #1d4d80; }
      .recommendation-pill.json { background: rgba(209, 73, 91, 0.12); color: #9d2338; }
      table {
        width: 100%;
        border-collapse: collapse;
        font-size: 0.95rem;
      }
      th, td {
        padding: 12px 10px;
        border-bottom: 1px solid var(--border);
        text-align: left;
      }
      thead th { font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
      tbody tr:last-child td { border-bottom: none; }
      .winner-row td:first-child { font-weight: 700; color: var(--good); }
      .failed-row td { color: var(--bad); }
      .check-table { margin-top: 18px; }
      .hero-metrics {
        display: grid;
        gap: 14px;
      }
      .hero-metrics .meta-card span { color: var(--muted); font-size: 0.85rem; }
      .chart-card svg { width: 100%; height: auto; display: block; }
      @media (max-width: 1080px) {
        .hero-grid, .summary-grid, .metrics-grid, .chart-grid {
          grid-template-columns: 1fr;
        }
        .dataset-header {
          flex-direction: column;
        }
      }
    </style>
  </head>
  <body>
    <main>
      <header class="hero">
        <div class="hero-grid">
          <div>
            <div class="eyebrow">Benchmark report</div>
            <h1>${PAGE_TITLE}</h1>
            <p class="lede">Scenario-heavy JSON vs TOON proof suite for POD world data. This report keeps the shell control plane on newline-delimited JSON and only recommends TOON where the measured dataset actually wins.</p>
            <ul class="meta-list">
              ${report.decision.shellRationale.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}
            </ul>
          </div>
          <div class="hero-metrics">
            <article class="meta-card">
              <span>Generated</span>
              <strong>${formatDate(report.generatedAtUnixMs)}</strong>
            </article>
            <article class="meta-card">
              <span>Iterations / rounds</span>
              <strong>${formatInteger(report.iterations)} / ${formatInteger(report.rounds)}</strong>
            </article>
            <article class="meta-card">
              <span>Checks</span>
              <strong>${report.checks.filter((check) => check.passed).length}/${report.checks.length} passing</strong>
            </article>
            <article class="meta-card">
              <span>Strict validation</span>
              <strong>${escapeHtml(report.validation.strictRowWidthError)}</strong>
            </article>
          </div>
        </div>
        <div class="summary-grid">
          ${summaryCards}
        </div>
      </header>

      <section class="panel">
        <div class="eyebrow">Overview charts</div>
        <h2>Winner deltas vs compact JSON</h2>
        <div class="chart-grid">
          <figure class="chart-card">${chartMap.get("overview-bytes-delta")?.svg ?? ""}</figure>
          <figure class="chart-card">${chartMap.get("overview-token-delta")?.svg ?? ""}</figure>
        </div>
      </section>

      <section class="panel">
        <div class="eyebrow">Per-dataset detail</div>
        ${datasetSections}
      </section>

      <section class="panel">
        <div class="eyebrow">Checks</div>
        <h2>Validation and decision gates</h2>
        <div class="table-card check-table">
          <table>
            <thead>
              <tr>
                <th>Metric</th>
                <th>Status</th>
                <th>Observed</th>
                <th>Expected</th>
              </tr>
            </thead>
            <tbody>
              ${checkRows}
            </tbody>
          </table>
        </div>
      </section>
    </main>
  </body>
</html>`;
}
