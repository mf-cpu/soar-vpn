<script>
  import { onMount, onDestroy } from "svelte";
  import uPlot from "uplot";
  import "uplot/dist/uPlot.min.css";
  import { trafficSeries } from "../lib/store.js";
  import { fmtBps } from "../lib/api.js";

  let container;
  let plot;
  let unsub;

  function fmtY(_, val) {
    return fmtBps(val);
  }

  function makeOpts(w, h) {
    return {
      width: w,
      height: h,
      legend: { show: false },
      cursor: { drag: { x: false, y: false } },
      scales: {
        x: { time: true },
        y: { auto: true, range: (_, min, max) => [0, Math.max(max, 1024)] },
      },
      axes: [
        {
          stroke: "#6a7088",
          grid: { stroke: "rgba(255,255,255,0.04)" },
        },
        {
          stroke: "#6a7088",
          values: (_, vals) => vals.map((v) => fmtBps(v)),
          grid: { stroke: "rgba(255,255,255,0.04)" },
          size: 70,
        },
      ],
      series: [
        {},
        {
          label: "下载",
          stroke: "#2ec27e",
          fill: "rgba(46,194,126,0.15)",
          width: 1.6,
          points: { show: false },
        },
        {
          label: "上传",
          stroke: "#5b8cff",
          fill: "rgba(91,140,255,0.15)",
          width: 1.6,
          points: { show: false },
        },
      ],
    };
  }

  function rebuild() {
    if (!container) return;
    if (plot) {
      plot.destroy();
      plot = null;
    }
    const w = container.clientWidth;
    const h = container.clientHeight;
    if (w < 10 || h < 10) return;
    plot = new uPlot(makeOpts(w, h), [[0], [0], [0]], container);
  }

  onMount(() => {
    rebuild();
    const ro = new ResizeObserver(() => {
      if (plot) plot.setSize({ width: container.clientWidth, height: container.clientHeight });
    });
    ro.observe(container);
    unsub = trafficSeries.subscribe((s) => {
      if (!plot) return;
      const t = s.t.length ? s.t : [Date.now() / 1000];
      const rx = s.rx.length ? s.rx : [0];
      const tx = s.tx.length ? s.tx : [0];
      plot.setData([t, rx, tx]);
    });
    return () => ro.disconnect();
  });

  onDestroy(() => {
    if (unsub) unsub();
    if (plot) plot.destroy();
  });
</script>

<div class="chart" bind:this={container}></div>

<style>
  .chart {
    width: 100%;
    height: 200px;
  }
  :global(.uplot) {
    background: transparent;
  }
</style>
