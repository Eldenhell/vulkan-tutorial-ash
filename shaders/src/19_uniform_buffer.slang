struct VSInput
{
    float2 inPosition;
    float3 inColor;
}

struct UniformBuffer
{
    float4x4 model;
    float4x4 view;
    float4x4 proj;
}
ConstantBuffer<UniformBuffer> ubo;

struct VSOutput
{
    float3 color;
    float4 pos : SV_Position;
};

[shader("vertex")]
VSOutput vertMain(VSInput input)
{
    VSOutput output;
    output.pos = mul(ubo.proj, mul(ubo.view, mul(ubo.model, float4(input.inPosition, 0.0, 1.0))));
    output.color = input.inColor;
    return output;
}

[shader("fragment")]
float4 fragMain(VSOutput vertIn) : SV_Target
{
    float3 color = vertIn.color;
    return float4(color, 1.0);
}
