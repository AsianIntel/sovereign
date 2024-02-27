#define PI 3.1415926535897932384626433832795

struct PSInput {
    float4 position: SV_Position;
    float3 normal: NORMAL;
    float2 uv: TEXCOORD;
    float4 frag_pos: POSITION;
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
    float4 frag_pos = pos;
    pos = mul(view, pos);
    pos = mul(projection, pos);

    PSInput result;
    result.position = pos;
    result.normal = vertexBuffer[vertexID].normal.xyz;
    result.uv = vertexBuffer[vertexID].uv;
    result.frag_pos = frag_pos;
    return result;
}

float D_GGX(float NoH, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float f = (NoH * a2 - NoH) * NoH + 1.0;
    return a2 / (PI * f * f);
}

float3 F_Schlick(float u, float3 f0) {
    return f0 + ((float3(1.0, 1.0, 1.0) - f0) * pow(1.0 - u, 5.0));
}

float G_SchlickGGX(float NoV, float roughness) {
    float r = (roughness + 1.0);
    float k = (r*r) / 8.0;
    float num = NoV;
    float denom = NoV * (1.0 - k) + k;
    return num / denom;
}

float G_Smith(float NoV, float NoL, float roughness) {
    float ggx2 = G_SchlickGGX(NoV, roughness);
    float ggx1 = G_SchlickGGX(NoL, roughness);
    return ggx1 * ggx2;
}

float Fd_Lambert() {
    return 1.0 / PI;
}

float3 BRDF(float3 n, float3 v, float3 l, Material material) {
    float3 h = normalize(v + l);

    float NoV = abs(dot(n, v)) + 1e-5;
    float NoL = clamp(dot(n, l), 0.0, 1.0);
    float NoH = clamp(dot(n, h), 0.0, 1.0);
    float HoV = clamp(dot(h, v), 0.0, 1.0);

    float roughness = material.perceptual_roughness;
    float3 diffuseColor = (1.0 - material.metallic)  * material.base_color_factors.rgb;
    float3 f0 = 0.16 * material.reflectance * material.reflectance * (1.0 - material.metallic) + material.base_color_factors.rgb * material.metallic;

    float NDF = D_GGX(NoH, roughness);
    float3 G = G_Smith(NoV, NoL, roughness);
    float3 F = F_Schlick(HoV, f0);

    float3 num = NDF * G * F;
    float denom = 4.0 * NoV * NoL + 0.0001;
    float3 specular = num / denom;

    float3 diffuse = diffuseColor * Fd_Lambert();

    return material.base_color_factors.rgb * (diffuse + specular);
}

float4 PSMain(PSInput input): SV_Target {
    StructuredBuffer<Material> materialBuffer = ResourceDescriptorHeap[renderResource.materialBufferIndex];
    ConstantBuffer<ViewUniform> viewBuffer = ResourceDescriptorHeap[renderResource.viewBufferIndex];

    Material material = materialBuffer[renderResource.materialOffset];

    float3 result = BRDF(input.normal, normalize(viewBuffer.view_position.xyz - input.frag_pos.xyz), float3(-2.0, 2.0, -2.0), material);

    return float4(result, 1.0);
}