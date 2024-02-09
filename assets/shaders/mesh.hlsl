struct PSInput {
    float4 position: SV_Position;
    float3 normal: NORMAL;
    float4 color: COLOR;
    float2 uv: TEXCOORD;
};

struct Vertex {
    float3 position;
    float3 normal;
    float2 uv;
    float4 color;
};

struct Transform {
    float4x4 model;
};

struct ViewUniform {
    float4x4 projection;
    float4x4 view;
    float3 position;
};

struct Material {
    float4 base_color_factors;
    float2 metal_rough_factors;
};

struct RenderResources {
    uint vertexBufferIndex;
    uint transformBufferIndex;
    uint transformOffset;
    uint viewBufferIndex;
    uint materialBufferIndex;
};

ConstantBuffer<RenderResources> renderResource: register(b0);

PSInput VSMain(uint vertexID: SV_VertexID) {
    StructuredBuffer<Vertex> vertexBuffer = ResourceDescriptorHeap[renderResource.vertexBufferIndex];
    StructuredBuffer<Transform> transformBuffer = ResourceDescriptorHeap[renderResource.transformBufferIndex];
    ConstantBuffer<ViewUniform> viewBuffer = ResourceDescriptorHeap[renderResource.viewBufferIndex];

    float4x4 model = transformBuffer[renderResource.transformOffset].model;
    float4x4 view = viewBuffer.view;
    float4x4 projection = viewBuffer.projection;

    float4 pos = float4(vertexBuffer[vertexID].position, 1.0f);
    pos = mul(model, pos);
    pos = mul(view, pos);
    pos = mul(projection, pos);

    PSInput result;
    result.position = pos;
    result.normal = vertexBuffer[vertexID].normal;
    result.color = vertexBuffer[vertexID].color;
    result.uv = vertexBuffer[vertexID].uv;
    return result;
}

float4 PSMain(PSInput input): SV_Target {
    StructuredBuffer<Material> materialBuffer = ResourceDescriptorHeap[renderResource.materialBufferIndex];
    ConstantBuffer<ViewUniform> viewBuffer = ResourceDescriptorHeap[renderResource.viewBufferIndex];

    Material material = materialBuffer[0];

    float4 result = material.base_color_factors * input.color;

    return result;
}