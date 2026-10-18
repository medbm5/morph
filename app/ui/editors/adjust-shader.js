/* Photo adjustments on the GPU (WebGL2). Mirrors crates/morph-engines/src/image/adjust.rs
   line for line — keep them in sync; app/tests/adjust-parity.mjs checks ΔE < 2.

   const r = createAdjustRenderer(canvas);   r.setImage(imgOrBitmap);   r.render(params);
   r.readPixels() → Uint8Array RGBA, top row first. */
(function (global) {
  'use strict';
  const KEYS = ['exposure', 'contrast', 'saturation', 'vibrance', 'temperature', 'tint', 'detail', 'clarity', 'dehaze', 'grain', 'noise_reduction'];

  const VERT = `#version 300 es
in vec2 pos;
void main() { gl_Position = vec4(pos, 0.0, 1.0); }`;

  const FRAG = `#version 300 es
precision highp float;
precision highp int;
uniform sampler2D src;
uniform vec2 size;
uniform float wide;
uniform float exposure, contrast, saturation, vibrance, temperature, tint, detail, clarity, dehaze, grain, noise_reduction;
out vec4 outColor;

float s2l(float c) { return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4); }
float l2s(float c) { c = max(c, 0.0); return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055; }
float luma(vec3 c) { return dot(c, vec3(0.2126, 0.7152, 0.0722)); }

float hash2(uint x, uint y) {
  uint v = x * 1664525u + y * 1013904223u + 0x9E3779B9u;
  v ^= v >> 16; v *= 0x7FEB352Du; v ^= v >> 15; v *= 0x846CA68Bu; v ^= v >> 16;
  return float(v >> 8) / 16777216.0;
}

// p in pixel coordinates (texel centres at +0.5), top row = 0.
vec3 samp(vec2 p) { return texture(src, p / size).rgb; }

vec3 ring(vec2 p, float r, int taps) {
  vec3 acc = vec3(0.0);
  for (int i = 0; i < 12; i++) {
    if (i >= taps) break;
    float a = float(i) * 6.28318530718 / float(taps);
    acc += samp(p + vec2(cos(a), sin(a)) * r);
  }
  return acc / float(taps);
}

void main() {
  ivec2 ip = ivec2(int(gl_FragCoord.x), int(size.y) - 1 - int(gl_FragCoord.y));
  vec2 p = vec2(ip) + 0.5;
  vec4 src4 = texture(src, p / size);
  vec3 c = src4.rgb;
  vec3 near = (noise_reduction > 0.0 || detail > 0.0) ? ring(p, 1.0, 8) : c;
  vec3 far = (clarity > 0.0 || dehaze > 0.0) ? ring(p, wide, 12) : c;

  if (noise_reduction > 0.0) c = c + (near - c) * (noise_reduction * 0.8);
  if (detail > 0.0) c = c + detail * 1.5 * (c - near);
  if (clarity > 0.0) {
    float l = clamp(luma(c), 0.0, 1.0);
    float mid = 1.0 - (2.0 * l - 1.0) * (2.0 * l - 1.0);
    c = c + clarity * 0.8 * mid * (c - far);
  }
  if (dehaze > 0.0) {
    float m = min(far.r, min(far.g, far.b));
    float t = max(1.0 - 0.75 * dehaze * m, 0.1);
    c = (c - 0.95 * (1.0 - t)) / t;
  }

  c = clamp(c, 0.0, 1.0);
  vec3 lin = vec3(s2l(c.r), s2l(c.g), s2l(c.b));
  if (exposure != 0.0) lin *= pow(2.0, exposure);
  if (temperature != 0.0 || tint != 0.0) {
    float t = temperature * 0.25, g = tint * 0.2;
    lin = vec3(lin.r * (1.0 + t), lin.g * (1.0 - g), lin.b * (1.0 - t));
  }
  c = vec3(l2s(lin.r), l2s(lin.g), l2s(lin.b));
  if (contrast != 0.0) {
    float k = contrast > 0.0 ? 1.0 + contrast * 1.5 : 1.0 + contrast * 0.9;
    c = (c - 0.5) * k + 0.5;
  }
  if (saturation != 0.0 || vibrance != 0.0) {
    float l = luma(c);
    float sat = clamp(max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b)), 0.0, 1.0);
    float k = max(1.0 + saturation + vibrance * (1.0 - sat), 0.0);
    c = l + (c - l) * k;
  }
  if (grain > 0.0) c += (hash2(uint(ip.x), uint(ip.y)) - 0.5) * grain * 0.16;
  outColor = vec4(clamp(c, 0.0, 1.0), src4.a);
}`;

  function createAdjustRenderer(canvas) {
    const gl = canvas.getContext('webgl2', { premultipliedAlpha: false, preserveDrawingBuffer: true, antialias: false });
    if (!gl) throw new Error('WebGL2 is not available');
    const sh = (type, src) => {
      const s = gl.createShader(type); gl.shaderSource(s, src); gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
      return s;
    };
    const prog = gl.createProgram();
    gl.attachShader(prog, sh(gl.VERTEX_SHADER, VERT));
    gl.attachShader(prog, sh(gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog));
    gl.useProgram(prog);
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(prog, 'pos');
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
    const tex = gl.createTexture();
    let w = 0, h = 0;

    return {
      maxSize: gl.getParameter(gl.MAX_TEXTURE_SIZE),
      setImage(img) {
        w = img.naturalWidth || img.width; h = img.naturalHeight || img.height;
        canvas.width = w; canvas.height = h;
        gl.bindTexture(gl.TEXTURE_2D, tex);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
        gl.pixelStorei(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL, gl.NONE);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      },
      render(params) {
        gl.viewport(0, 0, w, h);
        gl.uniform2f(gl.getUniformLocation(prog, 'size'), w, h);
        gl.uniform1f(gl.getUniformLocation(prog, 'wide'), Math.max(Math.min(w, h) * 0.015, 2));
        KEYS.forEach((k) => gl.uniform1f(gl.getUniformLocation(prog, k), +(params[k] || 0)));
        gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
      },
      readPixels() {
        const out = new Uint8Array(w * h * 4);
        gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, out);
        // GL rows are bottom-up.
        const row = w * 4, flipped = new Uint8Array(out.length);
        for (let y = 0; y < h; y++) flipped.set(out.subarray((h - 1 - y) * row, (h - y) * row), y * row);
        return flipped;
      },
      size: () => [w, h],
    };
  }

  global.createAdjustRenderer = createAdjustRenderer;
  global.ADJUST_KEYS = KEYS;
})(window);
