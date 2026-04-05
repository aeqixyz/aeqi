import { useEffect, useRef, useCallback } from "react";
import * as THREE from "three";

const RETURN_FORCE = 0.04;
const DAMPING = 0.88;
const MOUSE_RADIUS = 60;
const MOUSE_FORCE = 8;

function isMobile() {
  return /Android|iPhone|iPad|iPod/i.test(navigator.userAgent) || window.innerWidth < 768;
}

export default function ParticleLogo({
  width = 500,
  height = 200,
  size,
  onReady,
}: {
  width?: number;
  height?: number;
  size?: number;
  onReady?: () => void;
}) {
  const w = size ?? width;
  const h = size ?? height;
  const containerRef = useRef<HTMLDivElement>(null);
  const mouseRef = useRef({ x: 9999, y: 9999 });

  const handlePointerMove = useCallback((e: MouseEvent | TouchEvent) => {
    const el = containerRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const clientX = "touches" in e ? e.touches[0]?.clientX ?? 9999 : e.clientX;
    const clientY = "touches" in e ? e.touches[0]?.clientY ?? 9999 : e.clientY;
    mouseRef.current.x = clientX - rect.left - rect.width / 2;
    mouseRef.current.y = -(clientY - rect.top - rect.height / 2);
  }, []);

  const handlePointerLeave = useCallback(() => {
    mouseRef.current.x = 9999;
    mouseRef.current.y = 9999;
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const mobile = isMobile();
    const PARTICLE_COUNT = mobile ? 1200 : 2500;
    const dpr = Math.min(window.devicePixelRatio, mobile ? 2 : 2);

    const scene = new THREE.Scene();
    const camera = new THREE.OrthographicCamera(
      -w / 2, w / 2, h / 2, -h / 2, 1, 1000
    );
    camera.position.z = 100;

    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ alpha: true, antialias: !mobile });
    } catch {
      return;
    }
    renderer.setSize(w, h);
    renderer.setPixelRatio(dpr);
    container.appendChild(renderer.domElement);

    const positions = new Float32Array(PARTICLE_COUNT * 3);
    const targets = new Float32Array(PARTICLE_COUNT * 3);
    const velocities = new Float32Array(PARTICLE_COUNT * 3);
    const sizes = new Float32Array(PARTICLE_COUNT);
    const opacities = new Float32Array(PARTICLE_COUNT);

    for (let i = 0; i < PARTICLE_COUNT; i++) {
      positions[i * 3] = 0;
      positions[i * 3 + 1] = 0;
      positions[i * 3 + 2] = 0;
      velocities[i * 3] = 0;
      velocities[i * 3 + 1] = 0;
      velocities[i * 3 + 2] = 0;
      sizes[i] = 1.5 + Math.random() * 2.0;
      opacities[i] = 0.2 + Math.random() * 0.6;
    }

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute("aSize", new THREE.BufferAttribute(sizes, 1));
    geometry.setAttribute("aOpacity", new THREE.BufferAttribute(opacities, 1));

    const material = new THREE.ShaderMaterial({
      transparent: true,
      depthWrite: false,
      uniforms: {
        uPixelRatio: { value: renderer.getPixelRatio() },
      },
      vertexShader: `
        attribute float aSize;
        attribute float aOpacity;
        varying float vOpacity;
        uniform float uPixelRatio;
        void main() {
          vOpacity = aOpacity;
          vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
          gl_PointSize = aSize * uPixelRatio;
          gl_Position = projectionMatrix * mvPosition;
        }
      `,
      fragmentShader: `
        varying float vOpacity;
        void main() {
          float d = length(gl_PointCoord - vec2(0.5));
          if (d > 0.5) discard;
          float alpha = smoothstep(0.5, 0.1, d) * vOpacity;
          gl_FragColor = vec4(0.0, 0.0, 0.0, alpha * 0.7);
        }
      `,
    });

    const points = new THREE.Points(geometry, material);
    scene.add(points);

    function sampleGlyph(): Float32Array {
      const canvas = document.createElement("canvas");
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext("2d")!;
      ctx.fillStyle = "#000";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.font = `bold ${h * 0.7}px Inter, -apple-system, BlinkMacSystemFont, system-ui, sans-serif`;
      ctx.fillText("æqi", w / 2, h / 2 + h * 0.03);

      const imageData = ctx.getImageData(0, 0, w, h);
      const filled: [number, number][] = [];
      for (let y = 0; y < h; y += 1) {
        for (let x = 0; x < w; x += 1) {
          if (imageData.data[(y * w + x) * 4 + 3] > 128) {
            filled.push([x - w / 2, -(y - h / 2)]);
          }
        }
      }

      if (filled.length < 10) {
        for (let i = 0; i < 500; i++) {
          const angle = Math.random() * Math.PI * 2;
          const r = Math.random() * h * 0.3;
          filled.push([Math.cos(angle) * r, Math.sin(angle) * r]);
        }
      }

      const result = new Float32Array(PARTICLE_COUNT * 3);
      for (let i = 0; i < PARTICLE_COUNT; i++) {
        const pt = filled[Math.floor(Math.random() * filled.length)];
        result[i * 3] = pt[0] + (Math.random() - 0.5) * 2;
        result[i * 3 + 1] = pt[1] + (Math.random() - 0.5) * 2;
        result[i * 3 + 2] = 0;
      }
      return result;
    }

    // Wait for fonts then sample, with a timeout fallback
    function initParticles(glyphTargets: Float32Array) {
      for (let i = 0; i < PARTICLE_COUNT * 3; i++) {
        targets[i] = glyphTargets[i];
        positions[i] = glyphTargets[i];
      }
      (geometry.attributes.position as THREE.BufferAttribute).needsUpdate = true;
    }

    if (document.fonts?.ready) {
      document.fonts.ready.then(() => initParticles(sampleGlyph()));
    } else {
      // Fallback for browsers without font loading API
      setTimeout(() => initParticles(sampleGlyph()), 100);
    }

    let hasBurst = false;
    const BURST_FRAME = 3;
    const BURST_FORCE = 20;

    let frame = 0;
    let animId: number;

    // Pointer events — mouse + touch
    container.addEventListener("mousemove", handlePointerMove);
    container.addEventListener("mouseleave", handlePointerLeave);
    container.addEventListener("touchmove", handlePointerMove, { passive: true });
    container.addEventListener("touchend", handlePointerLeave);
    window.addEventListener("mousemove", handlePointerMove);

    function animate() {
      animId = requestAnimationFrame(animate);
      frame++;
      const time = frame * 0.008;

      if (!hasBurst) {
        if (frame === BURST_FRAME) {
          hasBurst = true;
          for (let i = 0; i < PARTICLE_COUNT; i++) {
            const angle = Math.random() * Math.PI * 2;
            const force = BURST_FORCE * (0.5 + Math.random());
            velocities[i * 3] = Math.cos(angle) * force;
            velocities[i * 3 + 1] = Math.sin(angle) * force;
          }
        } else {
          renderer.render(scene, camera);
          return;
        }
      }

      const mx = mouseRef.current.x;
      const my = mouseRef.current.y;
      const pos = geometry.attributes.position as THREE.BufferAttribute;

      for (let i = 0; i < PARTICLE_COUNT; i++) {
        const i3 = i * 3;

        const breathe = frame > BURST_FRAME + 60;
        const nx = breathe ? Math.sin(time * 0.6 + i * 0.07) * 0.15 : 0;
        const ny = breathe ? Math.cos(time * 0.5 + i * 0.09) * 0.15 : 0;

        const dx = targets[i3] - positions[i3];
        const dy = targets[i3 + 1] - positions[i3 + 1];

        velocities[i3] += dx * RETURN_FORCE + nx;
        velocities[i3 + 1] += dy * RETURN_FORCE + ny;

        const mdx = positions[i3] - mx;
        const mdy = positions[i3 + 1] - my;
        const mDist = Math.sqrt(mdx * mdx + mdy * mdy);
        if (mDist < MOUSE_RADIUS && mDist > 0.1) {
          const force = (1 - mDist / MOUSE_RADIUS) * MOUSE_FORCE;
          velocities[i3] += (mdx / mDist) * force;
          velocities[i3 + 1] += (mdy / mDist) * force;
        }

        velocities[i3] *= DAMPING;
        velocities[i3 + 1] *= DAMPING;

        positions[i3] += velocities[i3];
        positions[i3 + 1] += velocities[i3 + 1];
      }

      pos.needsUpdate = true;
      renderer.render(scene, camera);
    }

    animate();

    return () => {
      cancelAnimationFrame(animId);
      container.removeEventListener("mousemove", handlePointerMove);
      container.removeEventListener("mouseleave", handlePointerLeave);
      container.removeEventListener("touchmove", handlePointerMove);
      container.removeEventListener("touchend", handlePointerLeave);
      window.removeEventListener("mousemove", handlePointerMove);
      renderer.dispose();
      geometry.dispose();
      material.dispose();
      if (container.contains(renderer.domElement)) {
        container.removeChild(renderer.domElement);
      }
    };
  }, [w, h, onReady, handlePointerMove, handlePointerLeave]);

  return <div ref={containerRef} className="inline-block cursor-none" />;
}
