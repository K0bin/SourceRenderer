static float4 gl_Position;
static float3 out_color;
static float3 in_color;
static float3 in_pos;

struct SPIRV_Cross_Input
{
    float3 in_pos : POSITION;
    float3 in_color : COLOR;
};

struct SPIRV_Cross_Output
{
    float3 out_color : POSITION;
    float4 gl_Position : SV_Position;
};

void vert_main()
{
    out_color = in_color;
    gl_Position = float4(in_pos, 1.0f);
}

SPIRV_Cross_Output main(SPIRV_Cross_Input stage_input)
{
    in_color = stage_input.in_color;
    in_pos = stage_input.in_pos;
    vert_main();
    SPIRV_Cross_Output stage_output;
    stage_output.gl_Position = gl_Position;
    stage_output.out_color = out_color;
    return stage_output;
}
