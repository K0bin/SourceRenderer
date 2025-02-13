use std::{collections::HashMap, fs::File, io::Write, ops::Range, process::Command, u32};

use sourcerenderer_core::gpu::PER_SET_BINDINGS;

#[derive(Debug, Clone)]
struct Instruction {
    word_count: u16,
    opcode: u16
}

#[derive(Debug)]
struct OpTypePointer {
    result_id: u32,
    storage_class: u32,
    type_id: u32
}

#[derive(Debug)]
struct OpVariable {
    result_type_id: u32,
    result_id: u32,
    storage_class: u32,
    initializer: Option<u32>
}

#[derive(Debug)]
struct OpDecorate {
    target_id: u32,
    decoration_id: u32,
    value: Option<u32>
}

#[derive(Debug)]
struct OpMemberDecorate {
    structure_type: u32,
    member: u32,
    decoration_id: u32,
    value: Option<u32>
}

#[derive(Debug)]
struct OpTypeSampledImage {
    result_id: u32,
    image_type_id: u32
}

#[derive(Debug)]
struct OpTypeImage {
    result_id: u32,
    sampled_type_id: u32,
    dim: u32,
    depth: u32,
    arrayed: u32,
    ms: u32,
    sampled: u32,
    image_format: u32
}

#[derive(Debug)]
struct OpLoad {
    result_type_id: u32,
    result_id: u32,
    pointer_id: u32
}

#[derive(Debug)]
struct OpTypeFunction {
    result_id: u32,
    return_type_id: u32,
    parameters: Vec<u32>
}

struct OpFunctionParameter {
    result_type_id: u32,
    result_id: u32,
}

struct OpFunctionCall {
    result_type_id: u32,
    result_id: u32,
    function_id: u32,
    arguments: Vec<u32>
}

struct OpEntryPoint {
    execution_model: u32,
    entry_point_id: u32,
    name_string_id: u32,
    global_variable_ids: Vec<u32>
}

fn parse_instruction_description(word: u32) -> Instruction {
    Instruction {
        word_count: (word >> 16) as u16,
        opcode: (word & 0xFFFF) as u16
    }
}

fn build_instruction_description(instruction: &Instruction) -> u32 {
    ((instruction.word_count as u32) << 16) | instruction.opcode as u32
}

const OP_CODE_OP_DECORATE: u16 = 71;
const OP_CODE_OP_MEMBER_DECORATE: u16 = 72;
const OP_CODE_OP_TYPE_POINTER: u16 = 32;
const OP_CODE_OP_VARIABLE: u16 = 59;
const OP_CODE_OP_TYPE_IMAGE: u16 = 25;
const OP_CODE_OP_TYPE_SAMPLER: u16 = 26;
const OP_CODE_OP_TYPE_SAMPLED_IMAGE: u16 = 27;
const OP_CODE_OP_TYPE_FUNCTION: u16 = 33;
const OP_CODE_OP_FUNCTION: u16 = 54;
const OP_CODE_OP_FUNCTION_PARAMETER: u16 = 55;
const OP_CODE_OP_FUNCTION_CALL: u16 = 57;

const OP_CODE_OP_ENTRY_POINT: u16 = 15;

const OP_CODE_OP_LOAD: u16 = 61;
const OP_CODE_OP_SAMPLED_IMAGE: u16 = 86;

const OP_CODE_OP_SOURCE_CONTINUED: u16 = 2;
const OP_CODE_OP_SOURCE: u16 = 3;
const OP_CODE_OP_SOURCE_EXTENSION: u16 = 4;
const OP_CODE_OP_NAME: u16 = 5;
const OP_CODE_OP_MEMBER_NAME: u16 = 6;
const OP_CODE_OP_STRING: u16 = 7;
const OP_CODE_OP_LINE: u16 = 8;
const OP_CODE_OP_NO_LINE: u16 = 317;
const OP_CODE_OP_MODULE_PROCESSED: u16 = 330;

const DECORATION_LOCATION: u32 = 30;
const DECORATION_BINDING: u32 = 33;
const DECORATION_DESCRIPTOR_SET: u32 = 34;

const STORAGE_CLASS_UNIFORM_CONSTANT: u32 = 0;
const STORAGE_CLASS_FUNCTION: u32 = 7;
const STORAGE_CLASS_UNIFORM: u32 = 2;
const STORAGE_CLASS_PUSH_CONSTANT: u32 = 9;
fn parse_op_type_pointer(words: &[u32]) -> OpTypePointer {
    OpTypePointer {
        result_id: words[0],
        storage_class: words[1],
        type_id: words[2]
    }
}
fn parse_op_variable(words: &[u32]) -> OpVariable {
    OpVariable {
        result_type_id: words[0],
        result_id: words[1],
        storage_class: words[2],
        initializer: words.get(3).copied()
    }
}
fn parse_op_decorate(words: &[u32]) -> OpDecorate {
    OpDecorate {
        target_id: words[0],
        decoration_id: words[1],
        value: if words.len() >= 3 { Some(words[2]) } else { None }
    }
}
fn parse_op_member_decorate(words: &[u32]) -> OpMemberDecorate {
    OpMemberDecorate {
        structure_type: words[0],
        member: words[1],
        decoration_id: words[2],
        value: if words.len() >= 4 { Some(words[3]) } else { None }
    }
}
fn parse_op_type_sampled_image(words: &[u32]) -> OpTypeSampledImage {
    OpTypeSampledImage {
        result_id: words[0],
        image_type_id: words[1]
    }
}
fn parse_op_load(words: &[u32]) -> OpLoad {
    OpLoad {
        result_type_id: words[0],
        result_id: words[1],
        pointer_id: words[2]
    }
}
fn parse_op_type_image(words: &[u32]) -> OpTypeImage {
    OpTypeImage {
        result_id: words[0],
        sampled_type_id: words[1],
        dim: words[2],
        depth: words[3],
        arrayed: words[4],
        ms: words[5],
        sampled: words[6],
        image_format: words[7],
    }
}
fn parse_op_type_function(words: &[u32]) -> OpTypeFunction {
    let mut parameters = Vec::<u32>::new();
    for param in &words[2..] {
        parameters.push(*param);
    }
    OpTypeFunction {
        result_id: words[0],
        return_type_id: words[1],
        parameters
    }
}
fn parse_op_function_parameter(words: &[u32]) -> OpFunctionParameter {
    OpFunctionParameter {
        result_type_id: words[0],
        result_id: words[1]
    }
}
fn parse_op_function_call(words: &[u32]) -> OpFunctionCall {
    let mut arguments = Vec::<u32>::new();
    for argument in &words[3..] {
        arguments.push(*argument);
    }
    OpFunctionCall {
        result_type_id: words[0],
        result_id: words[1],
        function_id: words[2],
        arguments: arguments
    }
}
fn parse_op_entry_point(words: &[u32]) -> OpEntryPoint {
    let mut global_variable_ids = Vec::<u32>::new();
    for param in &words[3..] {
        global_variable_ids.push(*param);
    }
    OpEntryPoint {
        execution_model: words[0],
        entry_point_id: words[1],
        name_string_id: words[2],
        global_variable_ids
    }
}

fn cast_to_words<'a>(spirv: &'a mut [u8]) -> &'a mut [u32] {
    assert_eq!(spirv.len() % std::mem::size_of::<u32>(), 0);
    assert_eq!(spirv.as_ptr() as usize % std::mem::align_of::<u32>(), 0);
    unsafe {
        std::slice::from_raw_parts_mut(spirv.as_mut_ptr() as *mut u32, spirv.len() / 4)
    }
}

fn insert_words(spirv: &mut Vec<u8>, first_word_index: usize, words: &[u32]) {
    assert_eq!(spirv.len() % std::mem::size_of::<u32>(), 0);
    assert_eq!(spirv.capacity() % std::mem::size_of::<u32>(), 0);
    assert_eq!(spirv.as_ptr() as usize % std::mem::align_of::<u32>(), 0);
    for (word_index, word) in words.iter().enumerate() {
        for i in 0..std::mem::size_of::<u32>() {
            spirv.insert(
                (first_word_index + word_index) * std::mem::size_of::<u32>() + i,
                (word >> (8 * i)) as u8
            );
        }
    }
}

fn remove_words(spirv: &mut Vec<u8>, word_range: Range<usize>) {
    assert_eq!(spirv.len() % std::mem::size_of::<u32>(), 0);
    assert_eq!(spirv.capacity() % std::mem::size_of::<u32>(), 0);
    assert_eq!(spirv.as_ptr() as usize % std::mem::align_of::<u32>(), 0);

    let byte_start = word_range.start * std::mem::size_of::<u32>();
    let byte_end = word_range.end * std::mem::size_of::<u32>();
    spirv.drain(byte_start .. byte_end);
}

fn spirv_pass(spirv: &mut [u8], mut process_word: impl FnMut(usize, Instruction, &mut [u32]) -> bool) {
    let words = cast_to_words(spirv);

    let mut index = 0usize;
    assert_eq!(words[0], 0x07230203);
    index += 5;

    while index < words.len() {
        let word = words[index];
        let instruction = parse_instruction_description(word);

        assert_ne!(instruction.word_count, 0);
        let operand_words = &mut words[(index + 1)..(index + instruction.word_count as usize)];
        let word_count = instruction.word_count;
        let keep_iterating = process_word(index, instruction, operand_words);
        index += word_count as usize;
        if !keep_iterating {
            break;
        }
    }
}

pub fn spirv_turn_push_const_into_ubo_pass(spirv: &mut Vec<u8>, descriptor_set: u32, index: u32) {
    let mut first_type_word_index_opt = Option::<usize>::None;
    let mut target_id_opt = Option::<u32>::None;
    spirv_pass(&mut spirv[..], |word_index, instruction, operand_words| {
        if instruction.opcode == OP_CODE_OP_TYPE_POINTER {
            let ptr = parse_op_type_pointer(operand_words);
            if ptr.storage_class == STORAGE_CLASS_PUSH_CONSTANT {
                assert_eq!(operand_words[1], STORAGE_CLASS_PUSH_CONSTANT);
                operand_words[1] = STORAGE_CLASS_UNIFORM;
            }
            return true;
        }
        if instruction.opcode == OP_CODE_OP_VARIABLE {
            let var = parse_op_variable(operand_words);
            if var.storage_class == STORAGE_CLASS_PUSH_CONSTANT {
                assert_eq!(operand_words[2], STORAGE_CLASS_PUSH_CONSTANT);
                operand_words[2] = STORAGE_CLASS_UNIFORM;
                assert!(target_id_opt.is_none());
                target_id_opt = Some(var.result_id);
            }
            return true;
        }
        if first_type_word_index_opt.is_none()
            && ((instruction.opcode >= 19 && instruction.opcode <= 39)
                || instruction.opcode == 322
                || instruction.opcode == 327
                || instruction.opcode == 4456
                || instruction.opcode == 4472
                || instruction.opcode == 5281
                || instruction.opcode == 5358
                || instruction.opcode == 6086
                || instruction.opcode == 6090) {
            first_type_word_index_opt = Some(word_index);
        }
        return true;
    });

    if target_id_opt.is_none() {
        log::info!("Done replacing push constants");
        return;
    }

    let first_type_word_index = first_type_word_index_opt.unwrap();
    let target_id = target_id_opt.unwrap();

    insert_words(spirv, first_type_word_index, &[
        build_instruction_description(&Instruction {
            word_count: 4, opcode: OP_CODE_OP_DECORATE
        }),
        target_id,
        DECORATION_DESCRIPTOR_SET,
        descriptor_set,
        build_instruction_description(&Instruction {
            word_count: 4, opcode: OP_CODE_OP_DECORATE
        }),
        target_id,
        DECORATION_BINDING,
        index,
    ]);

    log::info!("Done replacing push constants");
}

pub fn spirv_remove_decoration(spirv: &mut Vec<u8>, decoration: u32) {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    spirv_pass(spirv, |word_index, instruction, operand_words| {
        if instruction.opcode == OP_CODE_OP_DECORATE {
            let decoration_instruction = parse_op_decorate(operand_words);
            if decoration_instruction.decoration_id == decoration {
                ranges.push(Range {
                    start: word_index, end: word_index + (instruction.word_count as usize)
                });
            }
        }
        if instruction.opcode == OP_CODE_OP_MEMBER_DECORATE {
            let decoration_instruction = parse_op_member_decorate(operand_words);
            if decoration_instruction.decoration_id == decoration {
                ranges.push(Range {
                    start: word_index, end: word_index + (instruction.word_count as usize)
                });
            }
        }
        return true;
    });
    ranges.reverse();
    for range in ranges {
        remove_words(spirv, range);
    }

    log::info!("Done removing decoration: {}", decoration);
}

pub fn spirv_remove_debug_info(spirv: &mut Vec<u8>) {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    spirv_pass(spirv, |word_index, instruction, _operand_words| {
        if instruction.opcode == OP_CODE_OP_SOURCE_CONTINUED
            || instruction.opcode == OP_CODE_OP_SOURCE
            || instruction.opcode == OP_CODE_OP_SOURCE_EXTENSION
            || instruction.opcode == OP_CODE_OP_NAME
            || instruction.opcode == OP_CODE_OP_MEMBER_NAME
            || instruction.opcode == OP_CODE_OP_STRING
            || instruction.opcode == OP_CODE_OP_LINE
            || instruction.opcode == OP_CODE_OP_NO_LINE
            || instruction.opcode == OP_CODE_OP_NAME
            || instruction.opcode == OP_CODE_OP_MODULE_PROCESSED {
            ranges.push(Range {
                start: word_index, end: word_index + (instruction.word_count as usize)
            });
        }
        return true;
    });
    ranges.reverse();
    for range in ranges {
        remove_words(spirv, range);
    }

    log::info!("Done removing debug info");
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub descriptor_set: u32,
    pub binding: u32
}
pub fn spirv_remap_bindings(spirv: &mut Vec<u8>, callback: impl Fn(&Binding) -> Binding) {
    let mut bindings = HashMap::<u32, Binding>::new();
    spirv_pass(spirv, |_word_index, instruction, operand_words| {
        if instruction.opcode != OP_CODE_OP_DECORATE {
            return true;
        }
        let decorate = parse_op_decorate(operand_words);
        if decorate.decoration_id == DECORATION_DESCRIPTOR_SET {
            let entry = bindings
                .entry(decorate.target_id)
                .or_insert(Binding {
                    descriptor_set: u32::MAX,
                    binding: u32::MAX
                });
            entry.descriptor_set = decorate.value.unwrap();
            if entry.descriptor_set != u32::MAX && entry.binding != u32::MAX {
                *entry = callback(entry);
            }
            return true;
        }
        if decorate.decoration_id == DECORATION_BINDING {
            let entry = bindings
                .entry(decorate.target_id)
                .or_insert(Binding {
                    descriptor_set: u32::MAX,
                    binding: u32::MAX
                });
            entry.binding = decorate.value.unwrap();
            if entry.descriptor_set != u32::MAX && entry.binding != u32::MAX {
                *entry = callback(entry);
            }
            return true;
        }
        return true;
    });
    spirv_pass(spirv, |_word_index, instruction, operand_words| {
        if instruction.opcode != OP_CODE_OP_DECORATE {
            return true;
        }
        let decorate = parse_op_decorate(operand_words);
        let binding_opt = bindings.get(&decorate.target_id);
        if binding_opt.is_none() {
            return true;
        }
        let binding = binding_opt.unwrap();
        match decorate.decoration_id {
            DECORATION_DESCRIPTOR_SET => {
                operand_words[2] = binding.descriptor_set;
            }
            DECORATION_BINDING => {
                operand_words[2] = binding.binding;
            }
            _ => {}
        }
        return true;
    });

    log::info!("Done remapping bindings");
}

pub struct ImageSamplerBindingPair {
    pub image: Binding,
    pub sampler: Binding
}
#[derive(Debug)]
struct SampledImageTypeMappings {
    pub image_type: u32,
    pub image_ptr_type: u32,
    pub image_ptr_var: u32,
    pub sampled_image_type: u32,
    pub sampled_image_ptr_var: u32,
    pub sampler_ptr_var: u32,
    pub image_binding: Option<Binding>
}
pub fn spirv_separate_combined_image_samplers(spirv: &mut Vec<u8>, decide_binding: Option<impl Fn(&Binding) -> Binding>) -> Vec<ImageSamplerBindingPair> {
    let mut sampled_img_types = Vec::<(usize, Instruction, OpTypeSampledImage)>::new();
    let mut img_types = Vec::<(usize, Instruction, OpTypeImage)>::new();
    let mut ptr_types = Vec::<(usize, Instruction, OpTypePointer)>::new();
    let mut function_types = Vec::<(usize, Instruction, OpTypeFunction)>::new();
    let mut function_parameters = Vec::<(usize, Instruction, OpFunctionParameter)>::new();
    let mut function_calls = Vec::<(usize, Instruction, OpFunctionCall)>::new();
    let mut vars = Vec::<(usize, Instruction, OpVariable)>::new();
    let mut loads = Vec::<(usize, Instruction, OpLoad)>::new();
    let mut sampling_ops_positions = Vec::<usize>::new();
    let mut entry_points = Vec::<(usize, Instruction, OpEntryPoint)>::new();
    let mut decorations = Vec::<(usize, Instruction, OpDecorate)>::new();
    let mut sampler_type = Option::<u32>::None;
    let mut sampler_ptr_type = Option::<u32>::None;

    let mut mappings = Vec::<SampledImageTypeMappings>::new();

    let mut binding_pairs = Vec::<ImageSamplerBindingPair>::new();

    let mut next_id = {
        let words = cast_to_words(spirv);
        assert_eq!(words[0], 0x07230203);
        words[3]
    };

    let mut insertions = Vec::<(usize, Vec<u32>)>::new();

    // Collect all the required info
    spirv_pass(spirv, |word_pos, instruction, operand_words| {
        if instruction.opcode == OP_CODE_OP_DECORATE {
            let decoration = parse_op_decorate(operand_words);
            decorations.push((word_pos, instruction, decoration));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_TYPE_IMAGE {
            let image = parse_op_type_image(operand_words);
            img_types.push((word_pos, instruction, image));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_TYPE_SAMPLED_IMAGE {
            let sampled_image = parse_op_type_sampled_image(operand_words);
            sampled_img_types.push((word_pos, instruction, sampled_image));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_TYPE_SAMPLER {
            assert!(sampler_type.is_none());
            sampler_type = Some(operand_words[0]);
        }
        if instruction.opcode == OP_CODE_OP_TYPE_POINTER {
            let ptr = parse_op_type_pointer(operand_words);
            ptr_types.push((word_pos, instruction, ptr));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_VARIABLE {
            let var = parse_op_variable(operand_words);
            vars.push((word_pos, instruction, var));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_LOAD {
            let load = parse_op_load(operand_words);
            loads.push((word_pos, instruction, load));
            return true;
        }
        if false
            || (instruction.opcode >= 87
                && instruction.opcode != 95
                && instruction.opcode <= 97)
            || instruction.opcode == 100
            || instruction.opcode == 105
            || (instruction.opcode >= 305
                && instruction.opcode != 313
                && instruction.opcode <= 315)
            || (instruction.opcode >= 4500
                && instruction.opcode <= 4503)
            || instruction.opcode == 5283 {
            sampling_ops_positions.push(word_pos);
        }
        if instruction.opcode == OP_CODE_OP_ENTRY_POINT {
            let entry_point = parse_op_entry_point(operand_words);
            entry_points.push((word_pos, instruction, entry_point));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_FUNCTION_PARAMETER {
            let param = parse_op_function_parameter(operand_words);
            function_parameters.push((word_pos, instruction, param));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_TYPE_FUNCTION {
            let function_type = parse_op_type_function(operand_words);
            function_types.push((word_pos, instruction, function_type));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_FUNCTION_CALL {
            let function_call = parse_op_function_call(operand_words);
            function_calls.push((word_pos, instruction, function_call));
            return true;
        }
        true
    });

    if sampled_img_types.is_empty() {
        log::info!("No combined image samplers found in shader.");
        return Vec::new();
    }

    let mut type_insertion_point = sampled_img_types.last()
        .map(|(pos, instruction, _)| *pos + instruction.word_count as usize)
        .unwrap();

    if sampler_type.is_none() {
        sampler_type = Some(next_id);
        next_id += 1;
        insertions.push((type_insertion_point, vec![
            build_instruction_description(&Instruction { word_count: 2, opcode: OP_CODE_OP_TYPE_SAMPLER }),
            sampler_type.unwrap()
        ]));
    }

    // Check if we already have a pointer type for samplers
    sampler_ptr_type = ptr_types.iter()
        .find(|(_, _, ptr_type)| ptr_type.type_id == sampler_type.unwrap())
        .map(|(_, _, ptr_type)| ptr_type.result_id);

    if sampler_ptr_type.is_none() {
        sampler_ptr_type = Some(next_id);
        next_id += 1;
        insertions.push((type_insertion_point, vec![
            build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_TYPE_POINTER }),
            sampler_ptr_type.unwrap(),
            STORAGE_CLASS_UNIFORM_CONSTANT,
            sampler_type.unwrap()
        ]));
    }

    for (var_pos, var_instruction, var) in &vars {
        let ptr_type_opt = ptr_types.iter().find(|(_, _, ptr_type)| ptr_type.result_id == var.result_type_id);
        if ptr_type_opt.is_none() {
            continue;
        }
        let (_, _, ptr_type) = ptr_type_opt.unwrap();

        let sampled_image_type_opt = sampled_img_types.iter().find(|(_, _, sampled_img_type)| sampled_img_type.result_id == ptr_type.type_id);
        if sampled_image_type_opt.is_none() {
            continue;
        }
        let (_, _, sampled_img_type) = sampled_image_type_opt.unwrap();

        let mut mapping: SampledImageTypeMappings = SampledImageTypeMappings {
            image_type: sampled_img_type.image_type_id,
            image_ptr_type: u32::MAX,
            image_ptr_var: u32::MAX,
            sampler_ptr_var: u32::MAX,
            sampled_image_type: sampled_img_type.result_id,
            sampled_image_ptr_var: var.result_id,
            image_binding: None
        };

        // Insert pointer type for image
        mapping.image_ptr_type = next_id;
        next_id += 1;
        insertions.push((*var_pos, vec![
            build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_TYPE_POINTER }),
            mapping.image_ptr_type,
            STORAGE_CLASS_UNIFORM_CONSTANT,
            mapping.image_type
        ]));

        // Turn the combined image sampler variable into one for an image
        mapping.image_ptr_var = std::mem::replace(&mut mapping.sampled_image_ptr_var, u32::MAX);
        {
            let words = cast_to_words(spirv);
            words[*var_pos + 1] = mapping.image_ptr_type;
        }

        // Insert var for sampler
        mapping.sampler_ptr_var = next_id;
        next_id += 1;
        insertions.push((*var_pos + var_instruction.word_count as usize, vec![
            build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_VARIABLE }),
            sampler_ptr_type.unwrap(),
            mapping.sampler_ptr_var,
            STORAGE_CLASS_UNIFORM_CONSTANT,
        ]));
        mappings.push(mapping);

        let last_mapping_mut = mappings.last_mut().unwrap();
        let binding_decoration_opt = decorations.iter().find(|(_, _, decoration)| decoration.target_id == last_mapping_mut.image_ptr_var && decoration.decoration_id == DECORATION_BINDING);
        let descriptor_set_decoration_opt = decorations.iter().find(|(_, _, decoration)| decoration.target_id == last_mapping_mut.image_ptr_var && decoration.decoration_id == DECORATION_DESCRIPTOR_SET);
        if binding_decoration_opt.is_none() {
            log::warn!("Found global variable for combined image sampler but no binding decoration");
            continue;
        }
        if descriptor_set_decoration_opt.is_none() {
            log::warn!("Found global variable for combined image sampler but no descriptor_set decoration");
        }
        let (_, _, binding_decoration) = binding_decoration_opt.unwrap();
        let (_, _, descriptor_set_decoration) = descriptor_set_decoration_opt.unwrap();
        if binding_decoration.value.is_none() {
            log::warn!("Found binding decoration for global variable for combined image sampler but no value.");
            continue;
        }
        if descriptor_set_decoration.value.is_none() {
            log::warn!("Found descriptor set decoration for global variable for combined image sampler but no value.");
            continue;
        }
        last_mapping_mut.image_binding = Some(Binding {
            descriptor_set: descriptor_set_decoration.value.unwrap(),
            binding: binding_decoration.value.unwrap()
        });
    }

    for (param_pos, param_instruction, param) in &function_parameters {
        let ptr_type_opt = ptr_types.iter().find(|(_, _, ptr_type)| ptr_type.result_id == param.result_type_id);
        if ptr_type_opt.is_none() {
            continue;
        }
        let (_, _, ptr_type) = ptr_type_opt.unwrap();

        let sampled_image_type_opt = sampled_img_types.iter().find(|(_, _, sampled_img_type)| sampled_img_type.result_id == ptr_type.type_id);
        if sampled_image_type_opt.is_none() {
            continue;
        }
        let (sampled_img_type_pos, _, sampled_img_type) = sampled_image_type_opt.unwrap();

        // Find equivalent var mapping
        let mut var_mapping_opt = mappings.iter().find(|m| m.image_ptr_type == sampled_img_type.result_id);
        if var_mapping_opt.is_none() {
            let mut new_base_mapping = SampledImageTypeMappings {
                image_type: sampled_img_type.image_type_id,
                image_ptr_type: u32::MAX,
                image_ptr_var: u32::MAX,
                sampler_ptr_var: u32::MAX,
                sampled_image_type: sampled_img_type.result_id,
                sampled_image_ptr_var: u32::MAX,
                image_binding: None
            };

            // Insert pointer type for image
            new_base_mapping.image_ptr_type = next_id;
            next_id += 1;
            insertions.push((*sampled_img_type_pos, vec![
                build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_TYPE_POINTER }),
                new_base_mapping.image_ptr_type,
                STORAGE_CLASS_UNIFORM_CONSTANT,
                new_base_mapping.image_type
            ]));
            mappings.push(new_base_mapping);
            var_mapping_opt = mappings.last();
        }
        let var_mapping = var_mapping_opt.unwrap();

        let mut mapping: SampledImageTypeMappings = SampledImageTypeMappings {
            image_type: sampled_img_type.image_type_id,
            image_ptr_type: var_mapping.image_ptr_type,
            image_ptr_var: u32::MAX,
            sampler_ptr_var: u32::MAX,
            sampled_image_type: sampled_img_type.result_id,
            sampled_image_ptr_var: param.result_id,
            image_binding: None
        };

        // Turn the combined image sampler variable into one for an image
        mapping.image_ptr_var = std::mem::replace(&mut mapping.sampled_image_ptr_var, u32::MAX);
        {
            let words = cast_to_words(spirv);
            words[*param_pos + 1] = mapping.image_ptr_type;
        }

        // Insert param for sampler
        mapping.sampler_ptr_var = next_id;
        next_id += 1;
        insertions.push((*param_pos + param_instruction.word_count as usize, vec![
            build_instruction_description(&Instruction { word_count: 3, opcode: OP_CODE_OP_FUNCTION_PARAMETER }),
            sampler_ptr_type.unwrap(),
            mapping.sampler_ptr_var
        ]));

        mappings.push(mapping);
    }

    for sample_op_pos in &sampling_ops_positions {
        let words = cast_to_words(spirv);
        let loaded_image = words[sample_op_pos + 3];
        let image_load_opt = loads.iter().find(|(_, _, load)| load.result_id == loaded_image);
        if image_load_opt.is_none() {
            continue;
        }
        let (image_load_pos, image_load_instruction, image_load) = image_load_opt.unwrap();
        let mapping_opt = mappings.iter().find(|m| m.image_ptr_var == image_load.pointer_id);
        if mapping_opt.is_none() {
            log::warn!("Found a load of a combined image sampler but cannot find the mapping for it");
            continue;
        }
        let mapping = mapping_opt.unwrap();

        let loaded_sampler_id = next_id;
        next_id += 1;

        let sampled_image_id = next_id;
        next_id += 1;

        insertions.push((*image_load_pos + (image_load_instruction.word_count as usize), vec![
            build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_LOAD }),
            sampler_type.unwrap(),
            loaded_sampler_id,
            mapping.sampler_ptr_var,
            build_instruction_description(&Instruction { word_count: 5, opcode: OP_CODE_OP_SAMPLED_IMAGE }),
            mapping.sampled_image_type,
            sampled_image_id,
            loaded_image,
            loaded_sampler_id
        ]));
        words[sample_op_pos + 3] = sampled_image_id;
    }

    // Add sampler to entry point
    for (entry_point_pos, entry_point_instruction, entry_point) in &entry_points {
        let words = cast_to_words(spirv);
        let mut new_instruction = entry_point_instruction.clone();
        for global_var_id in &entry_point.global_variable_ids {
            let mapping_opt = mappings.iter().find(|m| m.image_ptr_var == *global_var_id);
            if mapping_opt.is_none() {
                continue;
            }
            let mapping = mapping_opt.unwrap();
            insertions.push((entry_point_pos + entry_point_instruction.word_count as usize, vec![
                mapping.sampler_ptr_var
            ]));
            new_instruction.word_count += 1;
            break;
        }
        words[*entry_point_pos] = build_instruction_description(&new_instruction);
    }

    // Add sampler to function type parameters
    for (function_idx, instruction, function_type) in &function_types {
        let words = cast_to_words(spirv);
        let mut new_instruction = instruction.clone();
        for (param_idx, param) in function_type.parameters.iter().enumerate() {
            let mapping_opt = mappings.iter().find(|m| m.sampled_image_type == *param);
            if mapping_opt.is_none() {
                continue;
            }
            let mapping = mapping_opt.unwrap();
            words[function_idx + 3 + param_idx] = mapping.image_ptr_type;
            new_instruction.word_count += 1;
            insertions.push((*function_idx + 2 + param_idx + 1, vec![sampler_ptr_type.unwrap()]));
        }
        words[*function_idx] = build_instruction_description(&new_instruction);
    }

    // Add sampler to function call arguments
    for (function_idx, instruction, function_call) in &function_calls {
        let words = cast_to_words(spirv);
        let mut new_instruction = instruction.clone();
        for (arg_idx, arg) in function_call.arguments.iter().enumerate() {
            let mapping_opt = mappings.iter().find(|m| m.image_ptr_var == *arg);
            if mapping_opt.is_none() {
                continue;
            }
            let mapping = mapping_opt.unwrap();
            new_instruction.word_count += 1;
            insertions.push((*function_idx + 3 + arg_idx + 1, vec![mapping.sampler_ptr_var]));
        }
        words[*function_idx] = build_instruction_description(&new_instruction);
    }

    // Add binding decorations for the new samplers
    log::warn!("MAPPINGS: {:?}", &mappings);
    let insertion_point_opt = decorations.last().map(|(pos, instruction, _)| pos + instruction.word_count as usize);
    if let Some(insertion_point) = insertion_point_opt {
        mappings.sort_by_key(|m| if let Some(binding) = m.image_binding.as_ref() {
            1 + binding.descriptor_set * PER_SET_BINDINGS + binding.binding
        } else {
            0
        });
        for m in &mappings {
            if m.image_binding.is_none() {
                continue;
            }
            let image_binding = m.image_binding.clone().unwrap();
            let sampler_binding = if let Some(decide_binding_fn) = &decide_binding {
                decide_binding_fn(&image_binding)
            } else {
                let mut highest_bindings = [0u32; sourcerenderer_core::gpu::TOTAL_SET_COUNT as usize];
                for (_, _, set_decoration) in &decorations {
                    if set_decoration.decoration_id != DECORATION_DESCRIPTOR_SET || set_decoration.value.is_none() {
                        continue;
                    }
                    let binding_decoration_opt = decorations.iter()
                        .find(|(_, _, binding_decoration)| binding_decoration.decoration_id == DECORATION_BINDING && binding_decoration.target_id == set_decoration.target_id);
                    if binding_decoration_opt.is_none() {
                        continue;
                    }
                    let (_, _, binding_decoration) = binding_decoration_opt.unwrap();
                    if binding_decoration.value.is_none() {
                        continue;
                    }
                    let set_decoration_val = set_decoration.value.unwrap() as usize;
                    let binding_decoration_val = binding_decoration.value.unwrap();
                    highest_bindings[set_decoration_val]
                        = highest_bindings[set_decoration_val].max(binding_decoration_val);
                }

                highest_bindings[image_binding.descriptor_set as usize] += 1;
                Binding {
                    descriptor_set: image_binding.descriptor_set,
                    binding: highest_bindings[image_binding.descriptor_set as usize]
                }
            };
            insertions.push((insertion_point, vec![
                build_instruction_description(&Instruction {
                    word_count: 4,
                    opcode: OP_CODE_OP_DECORATE
                }),
                m.sampler_ptr_var,
                DECORATION_DESCRIPTOR_SET,
                sampler_binding.descriptor_set,
                build_instruction_description(&Instruction {
                    word_count: 4,
                    opcode: OP_CODE_OP_DECORATE
                }),
                m.sampler_ptr_var,
                DECORATION_BINDING,
                sampler_binding.binding,
            ]));

            binding_pairs.push(ImageSamplerBindingPair { image: image_binding, sampler: sampler_binding });
        }
    }

    // Collect loaded variables, the pointer type and the type the pointer points to
    /*for (_, _, load) in &loads {
        let sampled_img_type_opt = sampled_img_types.iter()
            .find(|(_, _, img)| img.result_id == load.result_type_id);
        if sampled_img_type_opt.is_none() {
            continue;
        }

        let mut ptr_opt = vars.iter()
            .find(|(_, _, v)| v.result_id == load.pointer_id)
            .and_then(|(_, _, v)|
                ptr_types.iter()
                    .find(|(_, _, type_id)| type_id.result_id == v.result_type_id)
            );


        if ptr_opt.is_none() {
            ptr_opt = function_parameters.iter()
                .find(|(_, _, p)| p.result_id == load.pointer_id)
                .and_then(|(_, _, p)|
                    ptr_types.iter()
                        .find(|(_, _, type_id)| type_id.result_id == p.result_type_id)
                );

            if ptr_opt.is_none() {
                log::error!("Cannot find pointer target for load of %{}", load.pointer_id);
                continue;
            }
        }

        let (_, _, ptr) = ptr_opt.unwrap();

        if ptr.storage_class != STORAGE_CLASS_UNIFORM_CONSTANT {
            log::error!("Separating combined image sampler types with the following storage class is not supported: {:?}", ptr.storage_class);
            continue;
        }
        let (_, _, sampled_image_type) = sampled_img_type_opt.unwrap();

        if let Some(mapping) = mappings.iter().find(|mapping| mapping.sampled_image_ptr_var == load.pointer_id) {
            assert_eq!(mapping.sampled_image_type, sampled_image_type.result_id);
            assert_eq!(mapping.sampled_image_ptr_type, ptr.result_id);
            assert_eq!(mapping.sampled_image_ptr_var, load.pointer_id);
        } else {
            mappings.push(SampledImageTypeMappings {
                image_type: sampled_image_type.image_type_id,
                image_ptr_type: 0,
                image_ptr_var: 0,
                sampled_image_type: sampled_image_type.result_id,
                sampled_image_ptr_type: ptr.result_id,
                sampled_image_ptr_var: load.pointer_id,
                sampler_type: 0,
                sampler_ptr_type: 0,
                sampler_ptr_var: 0,
            });
        }
    }

    // Sort by binding (important for picking sampler bind points later)
    mappings.sort_by_key(|mapping| {
        let (_, binding) = bindings.get(&mapping.sampled_image_ptr_var).unwrap();
        assert!(binding.descriptor_set < sourcerenderer_core::gpu::TOTAL_SET_COUNT);
        assert!(binding.binding < sourcerenderer_core::gpu::PER_SET_BINDINGS);
        binding.descriptor_set * sourcerenderer_core::gpu::PER_SET_BINDINGS + binding.binding
    });

    let mut insertions = Vec::<(usize, Vec<u32>)>::new();
    let mut word_count_increases = Vec::<usize>::new();

    for mapping in &mut mappings {
        let words = cast_to_words(spirv);

        // Change type of existing sampled image ptr, ptr var and load to image
        for (ptr_idx, _, ptr) in &mut ptr_types {
            if ptr.result_id != mapping.sampled_image_ptr_type {
                continue;
            }
            assert_eq!(words[*ptr_idx + 3], ptr.type_id);
            assert_ne!(mapping.image_type, 0);
            words[*ptr_idx + 3] = mapping.image_type;
            ptr.type_id = mapping.image_type;
        }

        for (load_idx, _, load) in &mut loads {
            if load.pointer_id != mapping.sampled_image_ptr_var {
                continue;
            }
            assert_eq!(load.result_type_id, words[*load_idx + 1]);
            words[*load_idx + 1] = mapping.image_type;
            load.result_type_id = mapping.image_type;
        }

        mapping.image_ptr_type = std::mem::take(&mut mapping.sampled_image_ptr_type);
        mapping.image_ptr_var = std::mem::take(&mut mapping.sampled_image_ptr_var);

        // Insert sampler, sampler ptr, sampler ptr var o
        let mut type_def_words = Vec::<u32>::new();

        // Find insertion point
        let (type_insertion_point, _, _) = ptr_types.iter().find(|(ptr_idx, _, ptr)| ptr.result_id == mapping.image_ptr_type).unwrap();

        // Declare sampler type and sampler pointer type if we havent already
        if let Some(sampler_type) = sampler_type {
            mapping.sampler_type = sampler_type;
        } else {
            mapping.sampler_type = next_id;
            sampler_type = Some(mapping.sampler_type);
            next_id += 1;
            type_def_words.push(build_instruction_description(&Instruction { word_count: 2, opcode: OP_CODE_OP_TYPE_SAMPLER }));
            type_def_words.push(mapping.sampler_type);
        }
        if let Some(sampler_ptr_type) = sampler_ptr_type {
            mapping.sampler_ptr_type = sampler_ptr_type;
        } else {
            mapping.sampler_ptr_type = next_id;
            sampler_ptr_type = Some(mapping.sampler_ptr_type);
            next_id += 1;
            type_def_words.push(build_instruction_description(&Instruction { word_count: 2, opcode: OP_CODE_OP_TYPE_SAMPLER }));
            type_def_words.push(mapping.sampler_type);
        }
        insertions.push((*type_insertion_point, type_def_words));

        // Insert variable definition or function parameter
        let mut var_words = Vec::<u32>::new();
        if let Some((var_idx, _, _)) = vars.iter().find(|(_, _, var)| var.result_id == mapping.image_ptr_var) {
            for word in &[
                build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_VARIABLE }),
                mapping.sampler_ptr_type,
                mapping.sampler_ptr_var,
                STORAGE_CLASS_UNIFORM_CONSTANT
            ] {
                var_words.push(*word);
            }
            insertions.push((*var_idx, var_words));
        } else if let Some((param_idx, _, _)) = function_parameters.iter().find(|(_, _, param)| param.result_id == mapping.image_ptr_var) {
            for word in &[
                build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_VARIABLE }),
                mapping.sampler_ptr_type,
                mapping.sampler_ptr_var,
                STORAGE_CLASS_UNIFORM_CONSTANT
            ] {
                var_words.push(*word);
            }
            insertions.push((*param_idx, var_words));
        }

        // Load sampler and create sampled image
        for (load_, instruction, load) in &loads {
            if load.pointer_id != mapping.image_ptr_var {
                continue;
            }
            let loaded_image_id = load.result_id;
            let loaded_sampler_id = next_id;
            next_id += 1;
            mapping.sampled_image_ptr_var = next_id;
            next_id += 1;

            insertions.push((load_ + (instruction.word_count as usize), vec![
                build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_LOAD }),
                mapping.sampler_type,
                loaded_sampler_id,
                mapping.sampler_ptr_var,
                build_instruction_description(&Instruction { word_count: 5, opcode: OP_CODE_OP_SAMPLED_IMAGE }),
                mapping.sampled_image_type,
                mapping.sampled_image_ptr_var,
                loaded_image_id,
                loaded_sampler_id
            ]));

            // Change sampled image id from sampling ids
            for idx in &sampling_ops_positions {
                if words[idx + 3] == loaded_image_id {
                    words[idx + 3] = mapping.sampled_image_ptr_var;
                }
            }
        }

        // Add sampler to function type parameters
        for (function_idx, instruction, function_type) in &function_types {
            for (param_idx, param) in function_type.parameters.iter().enumerate() {
                if *param == mapping.image_ptr_type {
                    let mut changed_instruction = instruction.clone();
                    changed_instruction.word_count += 1;
                    words[*function_idx] = build_instruction_description(&changed_instruction);
                    insertions.push((*function_idx + param_idx, vec![mapping.sampler_ptr_type]));
                }
            }
        }

        // Add sampler to function call arguments
        for (function_idx, instruction, function_call) in &function_calls {
            for (argument_idx, argument) in function_call.arguments.iter().enumerate() {
                if *argument == mapping.image_ptr_var {
                    let mut changed_instruction = instruction.clone();
                    changed_instruction.word_count += 1;
                    words[*function_idx] = build_instruction_description(&changed_instruction);
                    insertions.push((*function_idx + param_idx, vec![mapping.sampler_ptr_type]));
                }
            }
            
        }

        // Add sampler to entry point
        for entry_point_ in &entry_point_indices {
            let entry_point_instruction = parse_instruction_description(words[*entry_point_]);
            for word in &words[entry_point_ + 3 .. entry_point_ + entry_point_instruction.word_count as usize] {
                if *word == mapping.image_ptr_var {
                    insertions.push((entry_point_ + entry_point_instruction.word_count as usize, vec![
                        mapping.sampler_ptr_var
                    ]));
                    word_count_increases.push(*entry_point_);
                    break;
                }
            }
            words[*entry_point_] = build_instruction_description(&entry_point_instruction);
        }

        // Find highest binding for descriptor set of image and add decorations for the sampler 1 above that (the binding before is important for consistency)
        let (binding_idx, binding) = bindings.get(&mapping.image_ptr_var).unwrap();
        assert_eq!(words[binding_idx + 1], mapping.image_ptr_var);

        let sampler_binding = if let Some(callback) = decide_binding.as_ref() {
            callback(binding)
        } else {
            highest_bindings[binding.descriptor_set as usize] += 1;
            Binding {
                descriptor_set: binding.descriptor_set,
                binding: highest_bindings[binding.descriptor_set as usize]
            }
        };
        let previous_decoration_instruction = parse_instruction_description(words[*binding_idx]);
        insertions.push((*binding_idx + previous_decoration_instruction.word_count as usize, vec![
            build_instruction_description(&Instruction {
                word_count: 4,
                opcode: OP_CODE_OP_DECORATE
            }),
            mapping.sampler_ptr_var,
            DECORATION_DESCRIPTOR_SET,
            sampler_binding.descriptor_set,
            build_instruction_description(&Instruction {
                word_count: 4,
                opcode: OP_CODE_OP_DECORATE
            }),
            mapping.sampler_ptr_var,
            DECORATION_BINDING,
            sampler_binding.binding,
        ]));
        result.push(ImageSamplerPair {
            image: binding.clone(),
            sampler: sampler_binding
        });
    }*/

    // Insert prepared words
    // Has to be done at the end to avoid screwing up collected indices
    let mut insertion_offset = 0usize;
    insertions.sort_by_key(|(idx, _)| *idx);
    for (insertion_index, words) in insertions {
        insert_words(spirv, insertion_index + insertion_offset, &words);
        insertion_offset += words.len();
    }

    // Increase max id
    {
        let words = cast_to_words(spirv);
        words[3] = next_id;
    }

    log::info!("Done separating combined image samplers");

    binding_pairs
}

pub fn spirv_validate(spirv: &[u8]) -> Result<(), String> {
    {
        let mut file = File::create("tmp.spv").unwrap();
        let _ = file.write_all(spirv);
        let _ = file.flush();
    }

    let mut command = Command::new("spirv-val");
    command
        .arg("tmp.spv");

    let output_res = command.output();
    //let _ = std::fs::remove_file("tmp.spv");
    match &output_res {
        Err(e) => {
            return Err(e.to_string());
        },
        Ok(output) => {
            if !output.status.success() {
                return Err(std::str::from_utf8(&output.stdout).unwrap().to_string());
            }
            return Ok(());
        }
    }
}
