use std::{collections::HashMap, ops::Range, u32};

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
const OP_CODE_OP_TYPE_POINTER: u16 = 32;
const OP_CODE_OP_TYPE_VARIABLE: u16 = 59;
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

        if instruction.word_count == 0 {
            println!("0 WORDS: index: {}, instruction: {:?}", index, &instruction);
        }

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
        if instruction.opcode == OP_CODE_OP_TYPE_VARIABLE {
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
        println!("Done replacing push constants");
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
        DECORATION_LOCATION,
        index,
    ]);

    println!("Done replacing push constants");
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

    println!("Done removing debug info");
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

    println!("Done remapping bindings");
}

pub struct ImageSamplerPair {
    pub image: Binding,
    pub sampler: Binding
}
#[derive(Debug)]
struct SampledImageTypeMappings {
    pub image_type: u32,
    pub image_type_ptr_type: u32,
    pub image_type_ptr_var: u32,
    pub sampled_image_type: u32,
    pub sampled_image_type_ptr_type: u32,
    pub sampled_image_type_ptr_var: u32,
    pub sampler_type: u32,
    pub sampler_type_ptr_type: u32,
    pub sampler_type_ptr_var: u32,
}
pub fn spirv_separate_combined_image_samplers(spirv: &mut Vec<u8>, decide_binding: Option<impl Fn(&Binding) -> Binding>) -> Vec<ImageSamplerPair> {
    let mut sampled_img_types = Vec::<(usize, Instruction, OpTypeSampledImage)>::new();
    let mut img_types = Vec::<(usize, Instruction, OpTypeImage)>::new();
    let mut ptr_types = Vec::<(usize, Instruction, OpTypePointer)>::new();
    let mut function_types = Vec::<(usize, Instruction, OpTypeFunction)>::new();
    let mut function_parameters = Vec::<(usize, Instruction, OpFunctionParameter)>::new();
    let mut function_calls = Vec::<(usize, Instruction, OpFunctionCall)>::new();
    let mut vars = Vec::<(usize, Instruction, OpVariable)>::new();
    let mut loads = Vec::<(usize, Instruction, OpLoad)>::new();
    let mut sampling_ops_indices = Vec::<usize>::new();
    let mut entry_point_indices = Vec::<usize>::new();
    let mut bindings = HashMap::<u32, (usize, Binding)>::new();
    let mut sampler_type = Option::<u32>::None;

    let mut mappings = Vec::<SampledImageTypeMappings>::new();
    let mut highest_bindings = [0u32; sourcerenderer_core::gpu::TOTAL_SET_COUNT as usize];

    let mut result = Vec::<ImageSamplerPair>::new();

    let mut next_id = {
        let words = cast_to_words(spirv);
        assert_eq!(words[0], 0x07230203);
        words[3]
    };

    // Collect all the required info

    spirv_pass(spirv, |word_index, instruction, operand_words| {
        if instruction.opcode == OP_CODE_OP_DECORATE {
            let decoration = parse_op_decorate(operand_words);
            if decoration.decoration_id == DECORATION_DESCRIPTOR_SET {
                let entry = bindings
                    .entry(decoration.target_id)
                    .or_insert((word_index, Binding { descriptor_set: u32::MAX, binding: u32::MAX }));
                entry.1.descriptor_set = decoration.value.unwrap();
                if entry.1.descriptor_set != u32::MAX && entry.1.binding != u32::MAX {
                    highest_bindings[entry.1.descriptor_set as usize] = highest_bindings[entry.1.descriptor_set as usize].max(entry.1.binding);
                }
            } else if decoration.decoration_id == DECORATION_BINDING {
                let entry = bindings
                    .entry(decoration.target_id)
                    .or_insert((word_index, Binding { descriptor_set: u32::MAX, binding: u32::MAX }));
                entry.1.binding = decoration.value.unwrap();
                if entry.1.descriptor_set != u32::MAX && entry.1.binding != u32::MAX {
                    highest_bindings[entry.1.descriptor_set as usize] = highest_bindings[entry.1.descriptor_set as usize].max(entry.1.binding);
                }
            } else {
                return true;
            }
        }
        if instruction.opcode == OP_CODE_OP_TYPE_IMAGE {
            let image = parse_op_type_image(operand_words);
            img_types.push((word_index, instruction, image));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_TYPE_SAMPLED_IMAGE {
            let sampled_image = parse_op_type_sampled_image(operand_words);
            sampled_img_types.push((word_index, instruction, sampled_image));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_TYPE_SAMPLER {
            assert!(sampler_type.is_none());
            sampler_type = Some(operand_words[0]);
        }
        if instruction.opcode == OP_CODE_OP_TYPE_POINTER {
            let ptr = parse_op_type_pointer(operand_words);
            ptr_types.push((word_index, instruction, ptr));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_TYPE_VARIABLE {
            let var = parse_op_variable(operand_words);
            if var.storage_class == STORAGE_CLASS_FUNCTION {
                return true;
            }
            vars.push((word_index, instruction, var));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_LOAD {
            let load = parse_op_load(operand_words);
            loads.push((word_index, instruction, load));
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
            sampling_ops_indices.push(word_index);
        }
        if instruction.opcode == OP_CODE_OP_ENTRY_POINT {
            entry_point_indices.push(word_index);
        }
        if instruction.opcode == OP_CODE_OP_FUNCTION_PARAMETER {
            let param = parse_op_function_parameter(operand_words);
            function_parameters.push((word_index, instruction, param));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_TYPE_FUNCTION {
            let function_type = parse_op_type_function(operand_words);
            function_types.push((word_index, instruction, function_type));
            return true;
        }
        if instruction.opcode == OP_CODE_OP_FUNCTION_CALL {
            let function_call = parse_op_function_call(operand_words);
            function_calls.push((word_index, instruction, function_call));
            return true;
        }

        true
    });

    for (function_idx, _, function_type) in &function_types {
        let ptr_opt = ptr_types.iter()
            .find(|(_, _, p)| p.result_id == function_type.return_type_id);
        if ptr_opt.is_none() {
            continue;
        }
        let (_, _, ptr) = ptr_opt.unwrap();

        let sampled_img_type_opt = ptr_opt.and_then(|(_, _, ptr)|
            sampled_img_types.iter()
                .find(|(_, _, img)| img.result_id == ptr.type_id));
        if sampled_img_type_opt.is_none() {
            continue;
        }
        let (_, _, sampled_image_type) = sampled_img_type_opt.unwrap();

        if let Some(mapping) = mappings.iter().find(|mapping| mapping.sampled_image_type_ptr_var == load.pointer_id) {
            assert_eq!(mapping.sampled_image_type, sampled_image_type.result_id);
            assert_eq!(mapping.sampled_image_type_ptr_type, ptr.result_id);
            assert_eq!(mapping.sampled_image_type_ptr_var, load.pointer_id);
        } else {
            mappings.push(SampledImageTypeMappings {
                image_type: sampled_image_type.image_type_id,
                image_type_ptr_type: 0,
                image_type_ptr_var: 0,
                sampled_image_type: sampled_image_type.result_id,
                sampled_image_type_ptr_type: ptr.result_id,
                sampled_image_type_ptr_var: load.pointer_id,
                sampler_type: 0,
                sampler_type_ptr_type: 0,
                sampler_type_ptr_var: 0,
            });
        }

    }

    for (_, _, load) in &loads {
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
        }

        if ptr_opt.is_none() {
            continue;
        }
        let (_, _, ptr) = ptr_opt.unwrap();

        let sampled_img_type_opt = ptr_opt.and_then(|(_, _, ptr)|
            sampled_img_types.iter()
                .find(|(_, _, img)| img.result_id == ptr.type_id));
        if sampled_img_type_opt.is_none() {
            continue;
        }
        let (_, _, sampled_image_type) = sampled_img_type_opt.unwrap();

        if let Some(mapping) = mappings.iter().find(|mapping| mapping.sampled_image_type_ptr_var == load.pointer_id) {
            assert_eq!(mapping.sampled_image_type, sampled_image_type.result_id);
            assert_eq!(mapping.sampled_image_type_ptr_type, ptr.result_id);
            assert_eq!(mapping.sampled_image_type_ptr_var, load.pointer_id);
        } else {
            mappings.push(SampledImageTypeMappings {
                image_type: sampled_image_type.image_type_id,
                image_type_ptr_type: 0,
                image_type_ptr_var: 0,
                sampled_image_type: sampled_image_type.result_id,
                sampled_image_type_ptr_type: ptr.result_id,
                sampled_image_type_ptr_var: load.pointer_id,
                sampler_type: 0,
                sampler_type_ptr_type: 0,
                sampler_type_ptr_var: 0,
            });
        }
    }

    // Sort by binding (important for picking sampler bind points later)

    mappings.sort_by_key(|mapping| {
        let (_, binding) = bindings.get(&mapping.sampled_image_type_ptr_var).unwrap();
        assert!(binding.descriptor_set < sourcerenderer_core::gpu::TOTAL_SET_COUNT);
        assert!(binding.binding < sourcerenderer_core::gpu::PER_SET_BINDINGS);
        binding.descriptor_set * sourcerenderer_core::gpu::PER_SET_BINDINGS + binding.binding
    });

    let mut insertions = Vec::<(usize, Vec<u32>)>::new();
    let mut word_count_increases = Vec::<usize>::new();

    /*for (function_idx, _, function) in &mut function_types {
        let words = cast_to_words(spirv);
        for (param_idx_relative, param) in function.parameters.iter().enumerate() {
            let ptr_opt = ptr_types.iter().find(|(_, _, s)| s.result_id == *param);
            if ptr_opt.is_none() {
                continue;
            }
            let (ptr_idx, _, ptr) = ptr_opt.unwrap();
            let sampled_type_opt = sampled_img_types.iter().find(|(_, _, s)| ptr.type_id == *param);
            if sampled_type_opt.is_none() {
                continue;
            }
            let (_, _, sampled_type) = sampled_type_opt.unwrap();
            if ptr.storage_class != STORAGE_CLASS_FUNCTION {
                continue;
            }
            let new_ptr_id = next_id;
            next_id += 1;
            insertions.push((*ptr_idx, vec![
                build_instruction_description(&Instruction {
                    word_count: 3,
                    opcode: OP_CODE_OP_TYPE_POINTER
                }),
                new_ptr_id,
                STORAGE_CLASS_FUNCTION,
                sampled_type.result_id
            ]));
            words[*function_idx + 3 + param_idx_relative] = new_ptr_id;
        }
    }
    for (param_idx, _, param) in &mut function_parameters {
        let words = cast_to_words(spirv);
        let ptr_opt = ptr_types.iter().find(|(_, _, s)| s.result_id == param.result_type_id);
        if ptr_opt.is_none() {
            continue;
        }
        let (ptr_idx, _, ptr) = ptr_opt.unwrap();
        let sampled_type_opt = sampled_img_types.iter().find(|(_, _, s)| ptr.type_id == param.result_type_id);
        if sampled_type_opt.is_none() {
            continue;
        }
        let (_, _, sampled_type) = sampled_type_opt.unwrap();
        if ptr.storage_class != STORAGE_CLASS_FUNCTION {
            continue;
        }
        let new_ptr_id = next_id;
        next_id += 1;
        insertions.push((*ptr_idx, vec![
            build_instruction_description(&Instruction {
                word_count: 3,
                opcode: OP_CODE_OP_TYPE_POINTER
            }),
            new_ptr_id,
            STORAGE_CLASS_FUNCTION,
            sampled_type.result_id
        ]));
        words[*param_idx + 1] = new_ptr_id;
    }*/

    for mapping in &mut mappings {
        let words = cast_to_words(spirv);

        // Change type of existing sampled image ptr, ptr var and load to image

        for (ptr_idx, _, ptr) in &mut ptr_types {
            if ptr.result_id != mapping.sampled_image_type_ptr_type {
                continue;
            }
            assert_eq!(words[*ptr_idx + 3], ptr.type_id);
            assert_ne!(mapping.image_type, 0);
            words[*ptr_idx + 3] = mapping.image_type;
            ptr.type_id = mapping.image_type;
        }

        for (load_idx, _, load) in &mut loads {
            if load.pointer_id != mapping.sampled_image_type_ptr_var {
                continue;
            }
            println!("Changing the type of load: {:?} to %{}", load, mapping.image_type);
            assert_eq!(load.result_type_id, words[*load_idx + 1]);
            words[*load_idx + 1] = mapping.image_type;
            load.result_type_id = mapping.image_type;
        }

        mapping.image_type_ptr_type = std::mem::take(&mut mapping.sampled_image_type_ptr_type);
        mapping.image_type_ptr_var = std::mem::take(&mut mapping.sampled_image_type_ptr_var);

        // Insert sampler, sampler ptr, sampler ptr var
        let (var_idx, _, _) = vars.iter().find(|(_, _, var)| var.result_id == mapping.image_type_ptr_var).unwrap();

        mapping.sampler_type_ptr_type = next_id;
        next_id += 1;
        mapping.sampler_type_ptr_var = next_id;
        next_id += 1;

        let mut type_def_words = Vec::<u32>::new();
        if let Some(sampler_type) = sampler_type {
            mapping.sampler_type = sampler_type;
        } else {
            mapping.sampler_type = next_id;
            sampler_type = Some(mapping.sampler_type);
            next_id += 1;
            type_def_words.push(build_instruction_description(&Instruction { word_count: 2, opcode: OP_CODE_OP_TYPE_SAMPLER }));
            type_def_words.push(mapping.sampler_type);
        }
        for word in &[
            build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_TYPE_POINTER }),
            mapping.sampler_type_ptr_type,
            STORAGE_CLASS_UNIFORM_CONSTANT,
            mapping.sampler_type,
            build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_TYPE_VARIABLE }),
            mapping.sampler_type_ptr_type,
            mapping.sampler_type_ptr_var,
            STORAGE_CLASS_UNIFORM_CONSTANT
        ] {
            type_def_words.push(*word);
        }

        insertions.push((*var_idx, type_def_words));

        // Load sampler and create sampled image

        for (load_, instruction, load) in &loads {
            if load.pointer_id != mapping.image_type_ptr_var {
                continue;
            }
            let loaded_image_id = load.result_id;
            let loaded_sampler_id = next_id;
            next_id += 1;
            mapping.sampled_image_type_ptr_var = next_id;
            next_id += 1;

            insertions.push((load_ + (instruction.word_count as usize), vec![
                build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_LOAD }),
                mapping.sampler_type,
                loaded_sampler_id,
                mapping.sampler_type_ptr_var,
                build_instruction_description(&Instruction { word_count: 5, opcode: OP_CODE_OP_SAMPLED_IMAGE }),
                mapping.sampled_image_type,
                mapping.sampled_image_type_ptr_var,
                loaded_image_id,
                loaded_sampler_id
            ]));

            // Change sampled image id from sampling ids

            for idx in &sampling_ops_indices {
                if words[idx + 3] == loaded_image_id {
                    words[idx + 3] = mapping.sampled_image_type_ptr_var;
                }
            }
        }

        // Add sampler to entry point

        for entry_point_ in &entry_point_indices {
            let entry_point_instruction = parse_instruction_description(words[*entry_point_]);
            for word in &words[entry_point_ + 3 .. entry_point_ + entry_point_instruction.word_count as usize] {
                if *word == mapping.image_type_ptr_var {
                    insertions.push((entry_point_ + entry_point_instruction.word_count as usize, vec![
                        mapping.sampler_type_ptr_var
                    ]));
                    word_count_increases.push(*entry_point_);
                    break;
                }
            }
            words[*entry_point_] = build_instruction_description(&entry_point_instruction);
        }

        // Find highest binding for descriptor set of image and add decorations for the sampler 1 above that (the binding before is important for consistency)

        let (binding_idx, binding) = bindings.get(&mapping.image_type_ptr_var).unwrap();
        assert_eq!(words[binding_idx + 1], mapping.image_type_ptr_var);

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
            mapping.sampler_type_ptr_var,
            DECORATION_DESCRIPTOR_SET,
            sampler_binding.descriptor_set,
            build_instruction_description(&Instruction {
                word_count: 4,
                opcode: OP_CODE_OP_DECORATE
            }),
            mapping.sampler_type_ptr_var,
            DECORATION_BINDING,
            sampler_binding.binding,
        ]));
        result.push(ImageSamplerPair {
            image: binding.clone(),
            sampler: sampler_binding
        });
    }

    // Insert prepared words
    // Has to be done at the end to avoid screwing up collected indices

    let mut insertion_offset = 0usize;
    insertions.sort_by_key(|(idx, _)| *idx);
    for (insertion_index, words) in insertions {
        insert_words(spirv, insertion_index + insertion_offset, &words);
        insertion_offset += words.len();
    }
    {
        let words = cast_to_words(spirv);
        for word_count_increase_ in word_count_increases {
            let mut instruction = parse_instruction_description(words[word_count_increase_]);
            instruction.word_count += 1;
            words[word_count_increase_] = build_instruction_description(&instruction);
        }
    }

    // Increase max id
    {
        let words = cast_to_words(spirv);
        words[3] = next_id;
    }

    spirv_pass(spirv, |word_index, instruction, operand_words| {
        if instruction.word_count == 0 {
            println!("0 WORDS: {:?}", instruction);
        }
        true
    });

    println!("Done separating combined image samplers");

    result
}
