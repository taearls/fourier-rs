/**
 * WebGL renderer for the spectrum analyzer.
 *
 * Manages shaders, buffers, and draw calls. Designed for minimal GC pressure:
 * reuses pre-allocated buffers and avoids per-frame allocations.
 */

export type RenderMode = "line" | "filled" | "bars";

// ---------------------------------------------------------------------------
// Shaders
// ---------------------------------------------------------------------------

const VERTEX_SHADER_SRC = `
  attribute vec2 a_position;
  void main() {
    gl_PointSize = 4.0;
    gl_Position = vec4(a_position, 0.0, 1.0);
  }
`;

const FRAGMENT_SHADER_SRC = `
  precision mediump float;
  uniform vec4 u_color;
  void main() {
    gl_FragColor = u_color;
  }
`;

// ---------------------------------------------------------------------------
// Helper: compile shader
// ---------------------------------------------------------------------------

function compileShader(
  gl: WebGLRenderingContext,
  type: number,
  source: string,
): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) throw new Error("Failed to create shader");
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const info = gl.getShaderInfoLog(shader);
    gl.deleteShader(shader);
    throw new Error(`Shader compilation failed: ${info}`);
  }
  return shader;
}

// ---------------------------------------------------------------------------
// WebGLRenderer class
// ---------------------------------------------------------------------------

export class WebGLRenderer {
  private gl: WebGLRenderingContext;
  private program: WebGLProgram;
  private positionBuffer: WebGLBuffer;
  private peakBuffer: WebGLBuffer;
  private aPosition: number;
  private uColor: WebGLUniformLocation;

  constructor(canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl", {
      antialias: true,
      alpha: false,
      premultipliedAlpha: false,
    });
    if (!gl) throw new Error("WebGL not available");
    this.gl = gl;

    // Compile and link program
    const vs = compileShader(gl, gl.VERTEX_SHADER, VERTEX_SHADER_SRC);
    const fs = compileShader(gl, gl.FRAGMENT_SHADER, FRAGMENT_SHADER_SRC);
    const program = gl.createProgram();
    if (!program) throw new Error("Failed to create program");
    gl.attachShader(program, vs);
    gl.attachShader(program, fs);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(`Program link failed: ${gl.getProgramInfoLog(program)}`);
    }
    this.program = program;

    // Attribute and uniform locations
    this.aPosition = gl.getAttribLocation(program, "a_position");
    const uColor = gl.getUniformLocation(program, "u_color");
    if (!uColor) throw new Error("Failed to get u_color uniform");
    this.uColor = uColor;

    // Create reusable buffers
    const posBuffer = gl.createBuffer();
    if (!posBuffer) throw new Error("Failed to create position buffer");
    this.positionBuffer = posBuffer;

    const pkBuffer = gl.createBuffer();
    if (!pkBuffer) throw new Error("Failed to create peak buffer");
    this.peakBuffer = pkBuffer;

    // Setup
    gl.useProgram(program);
    gl.enableVertexAttribArray(this.aPosition);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  }

  /**
   * Resize the WebGL canvas and set the viewport to the plot area.
   *
   * @param width - Full canvas width in physical pixels.
   * @param height - Full canvas height in physical pixels.
   * @param marginLeft - Left margin for axis labels (physical pixels).
   * @param marginTop - Top margin (physical pixels).
   * @param marginRight - Right margin (physical pixels).
   * @param marginBottom - Bottom margin for frequency labels (physical pixels).
   */
  resize(
    width: number,
    height: number,
    marginLeft = 0,
    marginTop = 0,
    marginRight = 0,
    marginBottom = 0,
  ): void {
    const gl = this.gl;
    gl.canvas.width = width;
    gl.canvas.height = height;
    // WebGL viewport origin is bottom-left, so marginBottom maps to y offset
    const plotW = width - marginLeft - marginRight;
    const plotH = height - marginTop - marginBottom;
    gl.viewport(marginLeft, marginBottom, plotW, plotH);
  }

  /** Clear the entire canvas to the background color. */
  clear(): void {
    const gl = this.gl;
    // Temporarily set scissor to full canvas so clear covers everything
    gl.disable(gl.SCISSOR_TEST);
    gl.clearColor(0.04, 0.04, 0.04, 1.0); // #0a0a0a
    gl.clear(gl.COLOR_BUFFER_BIT);
  }

  /** Draw the spectrum curve or fill. */
  drawSpectrum(vertices: Float32Array, mode: RenderMode): void {
    const gl = this.gl;

    gl.bindBuffer(gl.ARRAY_BUFFER, this.positionBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.DYNAMIC_DRAW);
    gl.vertexAttribPointer(this.aPosition, 2, gl.FLOAT, false, 0, 0);

    // Accent color: #7c3aed (124, 58, 237) with alpha
    switch (mode) {
      case "line":
        gl.uniform4f(this.uColor, 0.486, 0.227, 0.929, 1.0);
        gl.drawArrays(gl.LINE_STRIP, 0, vertices.length / 2);
        break;
      case "filled":
        // Fill with semi-transparent accent
        gl.uniform4f(this.uColor, 0.486, 0.227, 0.929, 0.4);
        gl.drawArrays(gl.TRIANGLE_STRIP, 0, vertices.length / 2);
        break;
      case "bars":
        gl.uniform4f(this.uColor, 0.486, 0.227, 0.929, 0.8);
        gl.drawArrays(gl.TRIANGLES, 0, vertices.length / 2);
        break;
    }
  }

  /** Draw the spectrum line on top of a fill (for filled mode). */
  drawSpectrumLine(vertices: Float32Array): void {
    const gl = this.gl;

    gl.bindBuffer(gl.ARRAY_BUFFER, this.positionBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.DYNAMIC_DRAW);
    gl.vertexAttribPointer(this.aPosition, 2, gl.FLOAT, false, 0, 0);

    gl.uniform4f(this.uColor, 0.486, 0.227, 0.929, 1.0);
    gl.drawArrays(gl.LINE_STRIP, 0, vertices.length / 2);
  }

  /** Draw a time-domain waveform line (green oscilloscope style). */
  drawWaveform(vertices: Float32Array): void {
    if (vertices.length === 0) return;
    const gl = this.gl;

    gl.bindBuffer(gl.ARRAY_BUFFER, this.positionBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.DYNAMIC_DRAW);
    gl.vertexAttribPointer(this.aPosition, 2, gl.FLOAT, false, 0, 0);

    // Oscilloscope green: #22c55e (34, 197, 94)
    gl.uniform4f(this.uColor, 0.133, 0.773, 0.369, 1.0);
    gl.drawArrays(gl.LINE_STRIP, 0, vertices.length / 2);
  }

  /** Draw peak marker dots. */
  drawPeaks(peakVertices: Float32Array): void {
    if (peakVertices.length === 0) return;
    const gl = this.gl;

    gl.bindBuffer(gl.ARRAY_BUFFER, this.peakBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, peakVertices, gl.DYNAMIC_DRAW);
    gl.vertexAttribPointer(this.aPosition, 2, gl.FLOAT, false, 0, 0);

    // Peak markers in bright white-ish accent
    gl.uniform4f(this.uColor, 1.0, 1.0, 1.0, 0.9);
    gl.drawArrays(gl.POINTS, 0, peakVertices.length / 2);
  }

  /** Clean up WebGL resources. */
  dispose(): void {
    const gl = this.gl;
    gl.deleteBuffer(this.positionBuffer);
    gl.deleteBuffer(this.peakBuffer);
    gl.deleteProgram(this.program);
  }
}
