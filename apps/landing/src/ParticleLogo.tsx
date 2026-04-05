import { useEffect, useRef } from "react";
import * as THREE from "three";

const PARTICLE_COUNT = 800;
const DRIFT_SPEED = 0.3;
const RETURN_FORCE = 0.015;
const DAMPING = 0.92;

export default function ParticleLogo({ size = 300 }: { size?: number }) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const scene = new THREE.Scene();
    const camera = new THREE.OrthographicCamera(
      -size / 2, size / 2, size / 2, -size / 2, 1, 1000
    );
    camera.position.z = 100;

    const renderer = new THREE.WebGLRenderer({
      alpha: true,
      antialias: true,
    });
    renderer.setSize(size, size);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    // Particle state
    const positions = new Float32Array(PARTICLE_COUNT * 3);
    const targets = new Float32Array(PARTICLE_COUNT * 3);
    const velocities = new Float32Array(PARTICLE_COUNT * 3);
    const opacities = new Float32Array(PARTICLE_COUNT);

    // Init scattered
    for (let i = 0; i < PARTICLE_COUNT; i++) {
      positions[i * 3] = (Math.random() - 0.5) * size * 0.8;
      positions[i * 3 + 1] = (Math.random() - 0.5) * size * 0.8;
      positions[i * 3 + 2] = 0;
      velocities[i * 3] = 0;
      velocities[i * 3 + 1] = 0;
      velocities[i * 3 + 2] = 0;
      opacities[i] = 0.15 + Math.random() * 0.45;
    }

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute("opacity", new THREE.BufferAttribute(opacities, 1));

    const material = new THREE.ShaderMaterial({
      transparent: true,
      depthWrite: false,
      uniforms: {
        uPixelRatio: { value: renderer.getPixelRatio() },
      },
      vertexShader: `
        attribute float opacity;
        varying float vOpacity;
        uniform float uPixelRatio;
        void main() {
          vOpacity = opacity;
          vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
          gl_PointSize = 2.0 * uPixelRatio;
          gl_Position = projectionMatrix * mvPosition;
        }
      `,
      fragmentShader: `
        varying float vOpacity;
        void main() {
          float d = length(gl_PointCoord - vec2(0.5));
          if (d > 0.5) discard;
          float alpha = smoothstep(0.5, 0.2, d) * vOpacity;
          gl_FragColor = vec4(0.0, 0.0, 0.0, alpha * 0.6);
        }
      `,
    });

    const points = new THREE.Points(geometry, material);
    scene.add(points);

    // Sample points from the æ glyph using a canvas
    function sampleGlyph(): Float32Array {
      const canvas = document.createElement("canvas");
      const s = size;
      canvas.width = s;
      canvas.height = s;
      const ctx = canvas.getContext("2d")!;
      ctx.fillStyle = "#000";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.font = `bold ${s * 0.75}px Inter, system-ui, sans-serif`;
      ctx.fillText("æ", s / 2, s / 2 + s * 0.04);

      const imageData = ctx.getImageData(0, 0, s, s);
      const filled: [number, number][] = [];
      for (let y = 0; y < s; y += 2) {
        for (let x = 0; x < s; x += 2) {
          if (imageData.data[(y * s + x) * 4 + 3] > 128) {
            filled.push([x - s / 2, -(y - s / 2)]);
          }
        }
      }

      const result = new Float32Array(PARTICLE_COUNT * 3);
      for (let i = 0; i < PARTICLE_COUNT; i++) {
        const pt = filled[Math.floor(Math.random() * filled.length)];
        // Add slight jitter
        result[i * 3] = pt[0] + (Math.random() - 0.5) * 3;
        result[i * 3 + 1] = pt[1] + (Math.random() - 0.5) * 3;
        result[i * 3 + 2] = 0;
      }
      return result;
    }

    const glyphTargets = sampleGlyph();
    for (let i = 0; i < PARTICLE_COUNT * 3; i++) {
      targets[i] = glyphTargets[i];
    }

    // Animation
    let frame = 0;
    let animId: number;

    function animate() {
      animId = requestAnimationFrame(animate);
      frame++;
      const time = frame * 0.01;

      const pos = geometry.attributes.position as THREE.BufferAttribute;

      for (let i = 0; i < PARTICLE_COUNT; i++) {
        const i3 = i * 3;

        // Drift noise
        const nx = Math.sin(time + i * 0.1) * DRIFT_SPEED;
        const ny = Math.cos(time * 0.7 + i * 0.13) * DRIFT_SPEED;

        // Spring toward target
        const dx = targets[i3] - positions[i3];
        const dy = targets[i3 + 1] - positions[i3 + 1];

        velocities[i3] += dx * RETURN_FORCE + nx * 0.1;
        velocities[i3 + 1] += dy * RETURN_FORCE + ny * 0.1;

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
      renderer.dispose();
      geometry.dispose();
      material.dispose();
      container.removeChild(renderer.domElement);
    };
  }, [size]);

  return <div ref={containerRef} className="inline-block" />;
}
