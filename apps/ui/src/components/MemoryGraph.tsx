import { useCallback, useEffect, useRef, useState } from "react";

interface GraphNode {
  id: string;
  key: string;
  content: string;
  category: string;
  x: number;
  y: number;
  vx: number;
  vy: number;
  hotness: number;
}

interface GraphEdge {
  source: string;
  target: string;
  relation: string;
  strength: number;
}

interface Props {
  nodes: GraphNode[];
  edges: GraphEdge[];
  onSelect?: (node: GraphNode | null) => void;
  selectedId?: string | null;
}

const CATEGORY_COLORS: Record<string, string> = {
  fact: "#3b82f6",
  procedure: "#8b5cf6",
  preference: "#f59e0b",
  context: "#6b7280",
  evergreen: "#22c55e",
  decision: "#000000",
  insight: "#22c55e",
};

const RELATION_COLORS: Record<string, string> = {
  supports: "#22c55e",
  contradicts: "#ef4444",
  caused_by: "#f59e0b",
  derived_from: "#8b5cf6",
  supersedes: "#3b82f6",
  related_to: "#6b7280",
};

export default function MemoryGraph({ nodes, edges, onSelect, selectedId }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const simRef = useRef<GraphNode[]>([]);
  const edgesRef = useRef<GraphEdge[]>(edges);
  const animRef = useRef<number>(0);
  const dragRef = useRef<{ node: GraphNode; offsetX: number; offsetY: number } | null>(null);
  const hoverRef = useRef<GraphNode | null>(null);
  const [dimensions, setDimensions] = useState({ w: 800, h: 500 });

  // Initialize simulation nodes with positions.
  useEffect(() => {
    const cx = dimensions.w / 2;
    const cy = dimensions.h / 2;
    simRef.current = nodes.map((n, i) => ({
      ...n,
      x: cx + (n.x ? (n.x % 600) - 300 : Math.cos(i * 2.4) * 150),
      y: cy + (n.y ? (n.y % 400) - 200 : Math.sin(i * 2.4) * 150),
      vx: 0,
      vy: 0,
    }));
    edgesRef.current = edges;
  }, [nodes, edges, dimensions]);

  // Resize observer.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const parent = canvas.parentElement;
    if (!parent) return;

    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        if (width > 0 && height > 0) {
          setDimensions({ w: Math.floor(width), h: Math.floor(height) });
        }
      }
    });
    ro.observe(parent);
    return () => ro.disconnect();
  }, []);

  // Force simulation + render loop.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = dimensions.w * dpr;
    canvas.height = dimensions.h * dpr;
    canvas.style.width = `${dimensions.w}px`;
    canvas.style.height = `${dimensions.h}px`;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const nodeMap = new Map<string, GraphNode>();

    function tick() {
      const sim = simRef.current;
      const edgs = edgesRef.current;
      if (sim.length === 0) return;

      nodeMap.clear();
      for (const n of sim) nodeMap.set(n.id, n);

      const cx = dimensions.w / 2;
      const cy = dimensions.h / 2;

      // Forces.
      for (const n of sim) {
        // Center gravity.
        n.vx += (cx - n.x) * 0.001;
        n.vy += (cy - n.y) * 0.001;

        // Repulsion between all nodes.
        for (const m of sim) {
          if (n === m) continue;
          const dx = n.x - m.x;
          const dy = n.y - m.y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 1;
          if (dist < 200) {
            const force = 800 / (dist * dist);
            n.vx += (dx / dist) * force;
            n.vy += (dy / dist) * force;
          }
        }
      }

      // Spring forces for edges.
      for (const e of edgs) {
        const s = nodeMap.get(e.source);
        const t = nodeMap.get(e.target);
        if (!s || !t) continue;
        const dx = t.x - s.x;
        const dy = t.y - s.y;
        const dist = Math.sqrt(dx * dx + dy * dy) || 1;
        const ideal = 120;
        const force = (dist - ideal) * 0.005 * e.strength;
        const fx = (dx / dist) * force;
        const fy = (dy / dist) * force;
        s.vx += fx;
        s.vy += fy;
        t.vx -= fx;
        t.vy -= fy;
      }

      // Apply velocity with damping.
      for (const n of sim) {
        if (dragRef.current?.node === n) continue;
        n.vx *= 0.85;
        n.vy *= 0.85;
        n.x += n.vx;
        n.y += n.vy;
        // Bounds.
        n.x = Math.max(30, Math.min(dimensions.w - 30, n.x));
        n.y = Math.max(30, Math.min(dimensions.h - 30, n.y));
      }

      // Render.
      ctx!.clearRect(0, 0, dimensions.w, dimensions.h);

      // Draw edges.
      for (const e of edgs) {
        const s = nodeMap.get(e.source);
        const t = nodeMap.get(e.target);
        if (!s || !t) continue;
        ctx!.beginPath();
        ctx!.moveTo(s.x, s.y);
        ctx!.lineTo(t.x, t.y);
        ctx!.strokeStyle = RELATION_COLORS[e.relation] || "#d1d5db";
        ctx!.globalAlpha = 0.3 + e.strength * 0.4;
        ctx!.lineWidth = 1 + e.strength;
        ctx!.stroke();
        ctx!.globalAlpha = 1;
      }

      // Draw nodes.
      for (const n of sim) {
        const isSelected = n.id === selectedId;
        const isHovered = hoverRef.current === n;
        const radius = 6 + n.hotness * 8 + (isSelected ? 3 : 0);
        const color = CATEGORY_COLORS[n.category] || "#6b7280";

        // Glow for selected.
        if (isSelected) {
          ctx!.beginPath();
          ctx!.arc(n.x, n.y, radius + 4, 0, Math.PI * 2);
          ctx!.fillStyle = color + "30";
          ctx!.fill();
        }

        // Node circle.
        ctx!.beginPath();
        ctx!.arc(n.x, n.y, radius, 0, Math.PI * 2);
        ctx!.fillStyle = color;
        ctx!.globalAlpha = 0.4 + n.hotness * 0.6;
        ctx!.fill();
        ctx!.globalAlpha = 1;

        if (isSelected || isHovered) {
          ctx!.strokeStyle = color;
          ctx!.lineWidth = 2;
          ctx!.stroke();
        }

        // Label.
        if (isSelected || isHovered || n.hotness > 0.5 || sim.length < 30) {
          ctx!.font = "11px 'Inter', sans-serif";
          ctx!.fillStyle = "rgba(0,0,0,0.7)";
          ctx!.textAlign = "center";
          ctx!.fillText(n.key, n.x, n.y + radius + 14);
        }
      }

      animRef.current = requestAnimationFrame(tick);
    }

    animRef.current = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(animRef.current);
  }, [dimensions, selectedId]);

  // Hit test helper.
  const hitTest = useCallback((x: number, y: number): GraphNode | null => {
    const sim = simRef.current;
    for (let i = sim.length - 1; i >= 0; i--) {
      const n = sim[i];
      const r = 6 + n.hotness * 8 + 4;
      const dx = x - n.x;
      const dy = y - n.y;
      if (dx * dx + dy * dy < r * r) return n;
    }
    return null;
  }, []);

  const getCanvasPos = useCallback((e: React.MouseEvent) => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };
    const rect = canvas.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }, []);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    const pos = getCanvasPos(e);
    const node = hitTest(pos.x, pos.y);
    if (node) {
      dragRef.current = { node, offsetX: pos.x - node.x, offsetY: pos.y - node.y };
      onSelect?.(node);
    } else {
      onSelect?.(null);
    }
  }, [hitTest, getCanvasPos, onSelect]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    const pos = getCanvasPos(e);
    if (dragRef.current) {
      dragRef.current.node.x = pos.x - dragRef.current.offsetX;
      dragRef.current.node.y = pos.y - dragRef.current.offsetY;
      dragRef.current.node.vx = 0;
      dragRef.current.node.vy = 0;
    } else {
      const node = hitTest(pos.x, pos.y);
      hoverRef.current = node;
      const canvas = canvasRef.current;
      if (canvas) canvas.style.cursor = node ? "pointer" : "default";
    }
  }, [hitTest, getCanvasPos]);

  const handleMouseUp = useCallback(() => {
    dragRef.current = null;
  }, []);

  return (
    <canvas
      ref={canvasRef}
      className="memory-graph-canvas"
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
    />
  );
}
