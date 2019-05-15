struct VS_IN
{
    float3 position : POSITION;
    float3 color : COLOR;
};

struct VS_OUT
{
    float3 color : COLOR;
    float4 position : SV_Position;
};

VS_OUT main(VS_IN vs_in)
{
    VS_OUT vs_out;
    vs_out.position = float4(vs_in.position, 1.0f);
    vs_out.color = vs_in.color;
    return vs_out;
}
