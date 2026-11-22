const CAMERA_BYTES = 144;
// Pipeline, texture, first, count, first instance, instances, indexed, scene.
const DRAW_WORDS = 8;
const DEPTH = "depth32float";

const VERTEX_LAYOUT = Object.freeze([
  {
    arrayStride: 48,
    attributes: [
      { shaderLocation: 0, offset: 0, format: "float32x3" },
      { shaderLocation: 1, offset: 12, format: "float32x2" },
      { shaderLocation: 2, offset: 20, format: "float32x4" },
      { shaderLocation: 8, offset: 36, format: "float32x3" },
    ],
  },
  {
    arrayStride: 84,
    stepMode: "instance",
    attributes: [
      ...Array.from({ length: 5 }, (_, index) => ({
        shaderLocation: 3 + index, offset: index * 16, format: "float32x4",
      })),
      { shaderLocation: 9, offset: 80, format: "uint32" },
    ],
  },
]);

const BLEND = {
  color: { srcFactor: "src-alpha", dstFactor: "one-minus-src-alpha" },
  alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha" },
};

// Keeps the scene rectangle inside the live attachment if a resize lands
// between configure and draw.
const clampRect = (rect, full) => {
  const x = Math.max(0, Math.min(rect[0], full[2] - 1));
  const y = Math.max(0, Math.min(rect[1], full[3] - 1));
  return [x, y, Math.max(1, Math.min(rect[2], full[2] - x)),
    Math.max(1, Math.min(rect[3], full[3] - y))];
};

export async function openGpu(canvas, source) {
  if (!navigator.gpu) throw new Error("WebGPU is unavailable in this browser context");
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: "low-power" });
  if (!adapter) throw new Error("No browser graphics adapter");
  const device = await adapter.requestDevice();
  const context = canvas.getContext("webgpu");
  if (!context) throw new Error("Could not open a WebGPU canvas");
  const format = navigator.gpu.getPreferredCanvasFormat();
  const shader = device.createShaderModule({ code: source });
  const bindings = device.createBindGroupLayout({ entries: [
    { binding: 0, visibility: GPUShaderStage.VERTEX, buffer: { type: "uniform" } },
    { binding: 1, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
    { binding: 2, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
  ] });
  const layout = device.createPipelineLayout({ bindGroupLayouts: [bindings] });
  const makePipeline = (scene, writeDepth) => device.createRenderPipelineAsync({
    layout,
    vertex: { module: shader, entryPoint: scene ? "scene" : "overlay", buffers: VERTEX_LAYOUT },
    fragment: { module: shader, entryPoint: "paint", targets: [{ format, blend: BLEND }] },
    depthStencil: { format: DEPTH, depthWriteEnabled: writeDepth, depthCompare: scene ? "less-equal" : "always" },
  });
  const scene = await makePipeline(true, true);
  const readDepth = await makePipeline(true, false);
  const overlay = await makePipeline(false, false);
  const info = adapter.info;
  return new BrowserGpu(canvas, context, format, device, bindings, [scene, overlay, readDepth], info);
}

class BrowserGpu {
  constructor(canvas, context, format, device, bindings, pipelines, info) {
    this.canvas = canvas;
    this.context = context;
    this.device = device;
    this.bindings = bindings;
    this.pipelines = pipelines;
    this.maxTexture = device.limits.maxTextureDimension2D;
    this.maxBuffer = device.limits.maxBufferSize;
    this.adapterName = info?.description || info?.device || info?.vendor || "WebGPU";
    this.hardware = info ? !info.isFallbackAdapter : false;
    this.lost = false;
    this.error = null;
    this.camera = device.createBuffer({ size: CAMERA_BYTES, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    this.sampler = device.createSampler({ minFilter: "linear", magFilter: "linear", addressModeU: "repeat", addressModeV: "repeat" });
    context.configure({ device, format, alphaMode: "opaque" });
    device.lost.then(() => { this.lost = true; });
    device.addEventListener("uncapturederror", (event) => { this.error = event.error; });
  }

  get lossSignal() { return this.device.lost; }

  configure(width, height) {
    this.canvas.width = width;
    this.canvas.height = height;
    this.depth?.destroy();
    this.depth = this.device.createTexture({ size: [width, height], format: DEPTH, usage: GPUTextureUsage.RENDER_ATTACHMENT });
    this.depthView = this.depth.createView();
  }

  uploadGeometry(vertices, instances, indices, draws) {
    const allocate = (input, usage) => {
      const buffer = this.device.createBuffer({ size: input.byteLength,
        usage, mappedAtCreation: true });
      new Uint8Array(buffer.getMappedRange()).set(input);
      buffer.unmap();
      return buffer;
    };
    const stream = GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST;
    return new BrowserResident(allocate(vertices, stream), allocate(instances, stream),
      indices.length ? allocate(indices, GPUBufferUsage.INDEX) : null, draws.slice());
  }

  uploadTexture(resident, width, height, rgba) {
    const texture = this.device.createTexture({ size: [width, height], format: "rgba8unorm",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
    resident.textures.push(texture);
    this.device.queue.writeTexture({ texture }, rgba, { bytesPerRow: width * 4 }, [width, height]);
    resident.groups.push(this.device.createBindGroup({ layout: this.bindings, entries: [
      { binding: 0, resource: { buffer: this.camera } },
      { binding: 1, resource: texture.createView() },
      { binding: 2, resource: this.sampler },
    ] }));
  }

  patchBuffer(resident, instances, offset, bytes) {
    if (this.error) throw this.error;
    this.device.queue.writeBuffer(instances ? resident.instances : resident.vertices, offset, bytes);
  }

  patchTexture(resident, index, origin, size, rgba) {
    if (this.error) throw this.error;
    this.device.queue.writeTexture(
      { texture: resident.textures[index], origin: Array.from(origin) },
      rgba, { bytesPerRow: size[0] * 4 }, Array.from(size),
    );
  }

  draw(resident, view, clear, viewport) {
    if (this.error) throw this.error;
    if (view.length) this.device.queue.writeBuffer(this.camera, 0, view);
    const command = this.device.createCommandEncoder();
    const target = this.context.getCurrentTexture();
    const full = [0, 0, target.width, target.height];
    const scene = clampRect(viewport, full);
    const render = command.beginRenderPass({
      colorAttachments: [{ view: target.createView(), clearValue: Array.from(clear), loadOp: "clear", storeOp: "store" }],
      depthStencilAttachment: { view: this.depthView, depthClearValue: 1, depthLoadOp: "clear", depthStoreOp: "discard" },
    });
    render.setVertexBuffer(0, resident.vertices);
    render.setVertexBuffer(1, resident.instances);
    if (resident.indices) render.setIndexBuffer(resident.indices, "uint32");
    let lastPipeline = -1;
    let lastTexture = -1;
    let lastScene = null;
    for (let index = 0; index < resident.draws.length; index += DRAW_WORDS) {
      const pipeline = resident.draws[index];
      const texture = resident.draws[index + 1];
      const sceneGroup = resident.draws[index + 7] !== 0;
      if (pipeline !== lastPipeline || sceneGroup !== lastScene) {
        render.setPipeline(this.pipelines[pipeline]);
        // SceneNoDepth shares the overlay pipeline, so the group flag decides
        // the rectangle: canvas scene vs full-target editor chrome.
        const rect = sceneGroup ? scene : full;
        render.setViewport(rect[0], rect[1], rect[2], rect[3], 0, 1);
        render.setScissorRect(rect[0], rect[1], rect[2], rect[3]);
        lastPipeline = pipeline;
        lastScene = sceneGroup;
      }
      if (texture !== lastTexture) { render.setBindGroup(0, resident.groups[texture]); lastTexture = texture; }
      const count = resident.draws[index + 3];
      const instances = resident.draws[index + 5];
      const first = resident.draws[index + 2];
      const firstInstance = resident.draws[index + 4];
      if (resident.draws[index + 6]) render.drawIndexed(count, instances, first, 0, firstInstance);
      else render.draw(count, instances, first, firstInstance);
    }
    render.end();
    this.device.queue.submit([command.finish()]);
  }

  async waitAsync() {
    await this.device.queue.onSubmittedWorkDone();
    if (this.error) throw this.error;
    if (this.lost) throw new Error("WebGPU device lost");
  }

  closeDevice() {
    this.depth?.destroy();
    this.camera.destroy();
    this.context.unconfigure();
    this.device.destroy();
  }
}

class BrowserResident {
  constructor(vertices, instances, indices, draws) {
    this.vertices = vertices;
    this.instances = instances;
    this.indices = indices;
    this.draws = draws;
    this.textures = [];
    this.groups = [];
  }

  destroy() {
    this.vertices.destroy();
    this.instances.destroy();
    this.indices?.destroy();
    this.textures.forEach((texture) => texture.destroy());
  }
}
