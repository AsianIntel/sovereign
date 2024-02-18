#define PI 3.1415926535897932384626433832795

struct PSInput {
    float4 position: SV_Position;
    float3 normal: NORMAL;
    float2 uv: TEXCOORD;
};

struct Vertex {
    float4 position;
    float4 normal;
    float4 color;
    float2 uv;
    float2 pad;
};

struct Transform {
    float4x4 model;
};

struct ViewUniform {
    float4x4 projection;
    float4x4 view;
    float4 view_position;
};

struct Material {
    float4 base_color_factors;
    float perceptual_roughness;
    float metallic;
    float reflectance;
    float pad;
};

struct RenderResources {
    uint vertexBufferIndex;
    uint transformBufferIndex;
    uint transformOffset;
    uint viewBufferIndex;
    uint materialBufferIndex;
    uint materialOffset;
};

ConstantBuffer<RenderResources> renderResource: register(b0);

PSInput VSMain(uint vertexID: SV_VertexID) {
    StructuredBuffer<Vertex> vertexBuffer = ResourceDescriptorHeap[renderResource.vertexBufferIndex];
    StructuredBuffer<Transform> transformBuffer = ResourceDescriptorHeap[renderResource.transformBufferIndex];
    ConstantBuffer<ViewUniform> viewBuffer = ResourceDescriptorHeap[renderResource.viewBufferIndex];

    float4x4 model = transformBuffer[renderResource.transformOffset].model;
    float4x4 view = viewBuffer.view;
    float4x4 projection = viewBuffer.projection;

    float4 pos = float4(vertexBuffer[vertexID].position.xyz, 1.0f);
    pos = mul(model, pos);
    pos = mul(view, pos);
    pos = mul(projection, pos);

    PSInput result;
    result.position = pos;
    result.normal = vertexBuffer[vertexID].normal.xyz;
    result.uv = vertexBuffer[vertexID].uv;
    return result;
}

float D_GGX(float NoH, float a) {
    float a2 = a * a;
    float f = (NoH * a2 - NoH) * NoH + 1.0;
    return a2 / (PI * f * f);
}

float3 F_Schlick(float u, float3 f0) {
    return f0 + ((float3(1.0, 1.0, 1.0) - f0) * pow(1.0 - u, 5.0));
}

float V_SmithGGXCorrelated(float NoV, float NoL, float a) {
    float a2 = a * a;
    float GGLX = NoV * sqrt((-NoL * a2 + NoL) * NoL + a2);
    float GGXV = NoL * sqrt((-NoV * a2 + NoV) * NoV + a2);
    return 0.5 / (GGXV + GGLX);
}

float Fd_Lambert() {
    return 1.0 / PI;
}

float3 BRDF(float3 n, float3 v, float3 l, Material material) {
    float3 h = normalize(v + l);

    float NoV = abs(dot(n, v)) + 1e-5;
    float NoL = clamp(dot(n, l), 0.0, 1.0);
    float NoH = clamp(dot(n, h), 0.0, 1.0);
    float LoH = clamp(dot(l, h), 0.0, 1.0);

    float roughness = material.perceptual_roughness * material.perceptual_roughness;
    float3 diffuseColor = (1.0 - material.metallic)  * material.base_color_factors.rgb;
    float3 f0 = 0.16 * material.reflectance * material.reflectance * (1.0 - material.metallic) + material.base_color_factors.rgb * material.metallic;

    float D = D_GGX(NoH, roughness);
    float3 F = F_Schlick(LoH, f0);
    float V = V_SmithGGXCorrelated(NoV, NoL, roughness);

    float3 Fr = (D * V) * F;
    float3 Fd = diffuseColor * Fd_Lambert();

    return material.base_color_factors.rgb * (Fr + Fd);
}

float4 PSMain(PSInput input): SV_Target {
    StructuredBuffer<Material> materialBuffer = ResourceDescriptorHeap[renderResource.materialBufferIndex];
    ConstantBuffer<ViewUniform> viewBuffer = ResourceDescriptorHeap[renderResource.viewBufferIndex];

    Material material = materialBuffer[renderResource.materialOffset];

    float3 result = BRDF(input.normal, viewBuffer.view_position.xyz, float3(-2.0, 2.0, -2.0), material);

    return float4(result, 1.0);
}