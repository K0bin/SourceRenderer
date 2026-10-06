#![allow(unused)]

use std::{collections::HashMap, fs::File, io::Write, ops::Range, process::Command};

use sourcerenderer_core::gpu::PER_SET_BINDINGS;

#[derive(Debug, Clone)]
struct Instruction {
    word_count: u16,
    opcode: u16,
}

#[derive(Debug)]
struct OpTypePointer {
    result_id: u32,
    storage_class: u32,
    type_id: u32,
}

#[allow(unused)]
#[derive(Debug)]
struct OpVariable {
    result_type_id: u32,
    result_id: u32,
    storage_class: u32,
    initializer: Option<u32>,
}

#[derive(Debug)]
struct OpDecorate {
    target_id: u32,
    decoration_id: u32,
    value: Option<u32>,
}

#[allow(unused)]
#[derive(Debug)]
struct OpMemberDecorate {
    structure_type: u32,
    member: u32,
    decoration_id: u32,
    value: Option<u32>,
}

#[derive(Debug)]
struct OpTypeSampledImage {
    result_id: u32,
    image_type_id: u32,
}

#[allow(unused)]
#[derive(Debug)]
struct OpTypeImage {
    result_id: u32,
    sampled_type_id: u32,
    dim: u32,
    depth: u32,
    arrayed: u32,
    ms: u32,
    sampled: u32,
    image_format: u32,
}

#[allow(unused)]
#[derive(Debug)]
struct OpLoad {
    result_type_id: u32,
    result_id: u32,
    pointer_id: u32,
}

#[allow(unused)]
#[derive(Debug)]
struct OpTypeFunction {
    result_id: u32,
    return_type_id: u32,
    parameters: Vec<u32>,
}

struct OpFunctionParameter {
    result_type_id: u32,
    result_id: u32,
}

#[allow(unused)]
struct OpFunctionCall {
    result_type_id: u32,
    result_id: u32,
    function_id: u32,
    arguments: Vec<u32>,
}

#[allow(unused)]
struct OpEntryPoint {
    execution_model: u32,
    entry_point_id: u32,
    name_words: Vec<u32>,
    global_variable_ids: Vec<u32>,
}

fn parse_instruction_description(word: u32) -> Instruction {
    Instruction {
        word_count: (word >> 16) as u16,
        opcode: (word & 0xFFFF) as u16,
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
#[allow(unused)]
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

#[allow(unused)]
const DECORATION_LOCATION: u32 = 30;
const DECORATION_BINDING: u32 = 33;
const DECORATION_DESCRIPTOR_SET: u32 = 34;

const STORAGE_CLASS_UNIFORM_CONSTANT: u32 = 0;
#[allow(unused)]
const STORAGE_CLASS_FUNCTION: u32 = 7;
const STORAGE_CLASS_UNIFORM: u32 = 2;
const STORAGE_CLASS_PUSH_CONSTANT: u32 = 9;
fn parse_op_type_pointer(words: &[u32]) -> OpTypePointer {
    OpTypePointer {
        result_id: words[0],
        storage_class: words[1],
        type_id: words[2],
    }
}
fn parse_op_variable(words: &[u32]) -> OpVariable {
    OpVariable {
        result_type_id: words[0],
        result_id: words[1],
        storage_class: words[2],
        initializer: words.get(3).copied(),
    }
}
fn parse_op_decorate(words: &[u32]) -> OpDecorate {
    OpDecorate {
        target_id: words[0],
        decoration_id: words[1],
        value: if words.len() >= 3 {
            Some(words[2])
        } else {
            None
        },
    }
}
fn parse_op_member_decorate(words: &[u32]) -> OpMemberDecorate {
    OpMemberDecorate {
        structure_type: words[0],
        member: words[1],
        decoration_id: words[2],
        value: if words.len() >= 4 {
            Some(words[3])
        } else {
            None
        },
    }
}
fn parse_op_type_sampled_image(words: &[u32]) -> OpTypeSampledImage {
    OpTypeSampledImage {
        result_id: words[0],
        image_type_id: words[1],
    }
}
fn parse_op_load(words: &[u32]) -> OpLoad {
    OpLoad {
        result_type_id: words[0],
        result_id: words[1],
        pointer_id: words[2],
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
        parameters,
    }
}
fn parse_op_function_parameter(words: &[u32]) -> OpFunctionParameter {
    OpFunctionParameter {
        result_type_id: words[0],
        result_id: words[1],
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
        arguments: arguments,
    }
}
fn parse_op_entry_point(words: &[u32]) -> OpEntryPoint {
    let mut global_variable_ids = Vec::<u32>::new();
    let mut name_words = Vec::<u32>::new();
    while name_words.last() != Some(&0u32) {
        let word = words[2 + name_words.len()];
        name_words.push(word);
    }

    for param in &words[2 + name_words.len()..] {
        global_variable_ids.push(*param);
    }
    OpEntryPoint {
        execution_model: words[0],
        entry_point_id: words[1],
        name_words,
        global_variable_ids,
    }
}

fn cast_to_words<'a>(spirv: &'a mut [u8]) -> &'a mut [u32] {
    assert_eq!(spirv.len() % std::mem::size_of::<u32>(), 0);
    assert_eq!(spirv.as_ptr() as usize % std::mem::align_of::<u32>(), 0);
    unsafe { std::slice::from_raw_parts_mut(spirv.as_mut_ptr() as *mut u32, spirv.len() / 4) }
}

fn insert_words(spirv: &mut Vec<u8>, first_word_index: usize, words: &[u32]) {
    assert_eq!(spirv.len() % std::mem::size_of::<u32>(), 0);
    assert_eq!(spirv.capacity() % std::mem::size_of::<u32>(), 0);
    assert_eq!(spirv.as_ptr() as usize % std::mem::align_of::<u32>(), 0);
    for (word_index, word) in words.iter().enumerate() {
        for i in 0..std::mem::size_of::<u32>() {
            spirv.insert(
                (first_word_index + word_index) * std::mem::size_of::<u32>() + i,
                (word >> (8 * i)) as u8,
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
    spirv.drain(byte_start..byte_end);
}

fn spirv_pass(
    spirv: &mut [u8],
    mut process_word: impl FnMut(usize, Instruction, &mut [u32]) -> bool,
) {
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

pub fn spirv_remove_decoration(spirv: &mut Vec<u8>, decoration: u32) {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    spirv_pass(spirv, |word_index, instruction, operand_words| {
        if instruction.opcode == OP_CODE_OP_DECORATE {
            let decoration_instruction = parse_op_decorate(operand_words);
            if decoration_instruction.decoration_id == decoration {
                ranges.push(Range {
                    start: word_index,
                    end: word_index + (instruction.word_count as usize),
                });
            }
        }
        if instruction.opcode == OP_CODE_OP_MEMBER_DECORATE {
            let decoration_instruction = parse_op_member_decorate(operand_words);
            if decoration_instruction.decoration_id == decoration {
                ranges.push(Range {
                    start: word_index,
                    end: word_index + (instruction.word_count as usize),
                });
            }
        }
        return true;
    });
    ranges.reverse();
    for range in ranges {
        remove_words(spirv, range);
    }
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
            || instruction.opcode == OP_CODE_OP_MODULE_PROCESSED
        {
            ranges.push(Range {
                start: word_index,
                end: word_index + (instruction.word_count as usize),
            });
        }
        return true;
    });
    ranges.reverse();
    for range in ranges {
        remove_words(spirv, range);
    }
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub descriptor_set: u32,
    pub binding: u32,
}
pub fn spirv_remap_bindings(spirv: &mut Vec<u8>, callback: impl Fn(&Binding) -> Binding) {
    let mut bindings = HashMap::<u32, Binding>::new();
    spirv_pass(spirv, |_word_index, instruction, operand_words| {
        if instruction.opcode != OP_CODE_OP_DECORATE {
            return true;
        }
        let decorate = parse_op_decorate(operand_words);
        if decorate.decoration_id == DECORATION_DESCRIPTOR_SET {
            let entry = bindings.entry(decorate.target_id).or_insert(Binding {
                descriptor_set: u32::MAX,
                binding: u32::MAX,
            });
            entry.descriptor_set = decorate.value.unwrap();
            if entry.descriptor_set != u32::MAX && entry.binding != u32::MAX {
                *entry = callback(entry);
            }
            return true;
        }
        if decorate.decoration_id == DECORATION_BINDING {
            let entry = bindings.entry(decorate.target_id).or_insert(Binding {
                descriptor_set: u32::MAX,
                binding: u32::MAX,
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
}

#[allow(unused)]
pub struct ImageSamplerBindingPair {
    pub image: Binding,
    pub sampler: Binding,
}
#[derive(Debug)]
struct SampledImageTypeMappings {
    pub image_type: u32,
    pub image_ptr_type: u32,
    pub image_ptr_var: u32,
    pub sampled_image_type: u32,
    pub sampled_image_ptr_type: u32,
    pub sampled_image_ptr_var: u32,
    pub sampler_ptr_var: u32,
    pub image_binding: Option<Binding>,
}

#[allow(unused)]
pub fn spirv_validate(spirv: &[u8]) -> Result<(), String> {
    {
        let mut file = File::create("tmp.spv").unwrap();
        let _ = file.write_all(spirv);
        let _ = file.flush();
    }

    let mut command = Command::new("spirv-val");
    command.arg("tmp.spv");

    let output_res = command.output();
    //let _ = std::fs::remove_file("tmp.spv");
    match &output_res {
        Err(e) => {
            return Err(e.to_string());
        }
        Ok(output) => {
            if !output.status.success() {
                return Err(std::str::from_utf8(&output.stdout).unwrap().to_string());
            }
            return Ok(());
        }
    }
}
