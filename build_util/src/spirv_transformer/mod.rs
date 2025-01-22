use std::{collections::HashMap, ops::Range};

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
    value: u32
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
        value: words[2]
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

pub fn spirv_push_const_pass(spirv: &mut Vec<u8>, descriptor_set: u32, index: u32) {
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

#[derive(Clone)]
pub struct Binding {
    pub descriptor_set: u32,
    pub binding: u32
}
pub fn spirv_remap_bindings(spirv: &mut Vec<u8>, callback: impl Fn(Binding) -> Binding) {
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
                    descriptor_set: 0,
                    binding: 0
                });
            entry.descriptor_set = decorate.value;
            return true;
        }
        if decorate.decoration_id == DECORATION_BINDING {
            let entry = bindings
                .entry(decorate.target_id)
                .or_insert(Binding {
                    descriptor_set: 0,
                    binding: 0
                });
            entry.binding = decorate.value;
            return true;
        }
        return true;
    });
    for (_id, binding) in &mut bindings {
        *binding = callback(binding.clone());
    }
    spirv_pass(spirv, |_word_index, instruction, operand_words| {
        if instruction.opcode != OP_CODE_OP_DECORATE {
            return true;
        }
        let decorate = parse_op_decorate(operand_words);
        let binding = bindings.get_mut(&decorate.target_id).unwrap();
        if decorate.decoration_id == DECORATION_DESCRIPTOR_SET {
            assert_eq!(operand_words[2], decorate.value);
            operand_words[2] = binding.descriptor_set;
        }
        if decorate.decoration_id == DECORATION_BINDING {
            assert_eq!(operand_words[2], decorate.value);
            operand_words[2] = binding.binding;
        }
        return true;
    });
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
pub fn spirv_separate_combined_image_samplers(spirv: &mut Vec<u8>) -> Vec<ImageSamplerPair> {
    let mut sampled_img_types = Vec::<(usize, Instruction, OpTypeSampledImage)>::new();
    let mut img_types = Vec::<(usize, Instruction, OpTypeImage)>::new();
    let mut sampled_img_ptr_types = Vec::<(usize, Instruction, OpTypePointer)>::new();
    let mut sampled_img_ptr_vars = Vec::<(usize, Instruction, OpVariable)>::new();
    let mut sampled_img_ptr_var_loads = Vec::<(usize, Instruction, OpLoad)>::new();
    let mut sampling_ops_indices = Vec::<usize>::new();
    let mut entry_point_indices = Vec::<usize>::new();
    let mut decorations = Vec::<(usize, OpDecorate)>::new();

    let mut mappings = Vec::<SampledImageTypeMappings>::new();

    let mut next_id = {
        let words = cast_to_words(spirv);
        assert_eq!(words[0], 0x07230203);
        words[3]
    };

    // STEP 1: Collect all the required info

    spirv_pass(spirv, |word_index, instruction, operand_words| {
        if instruction.opcode == OP_CODE_OP_DECORATE {
            let decoration = parse_op_decorate(operand_words);
            decorations.push((word_index, decoration));
        }
        if instruction.opcode == OP_CODE_OP_TYPE_IMAGE {
            let image = parse_op_type_image(operand_words);
            img_types.push((word_index, instruction.clone(), image));
        }
        if instruction.opcode == OP_CODE_OP_TYPE_SAMPLED_IMAGE {
            let sampled_image = parse_op_type_sampled_image(operand_words);
            sampled_img_types.push((word_index, instruction.clone(), sampled_image));
        }
        if instruction.opcode == OP_CODE_OP_TYPE_POINTER {
            let ptr = parse_op_type_pointer(operand_words);
            let sampled_img_type = sampled_img_types.iter()
                .find(|(_idx, _, img_type)| img_type.result_id == ptr.type_id);
            if sampled_img_type.is_some() {
                sampled_img_ptr_types.push((word_index, instruction.clone(), ptr));
            }
        }
        if instruction.opcode == OP_CODE_OP_TYPE_VARIABLE {
            let var = parse_op_variable(operand_words);
            let ptr_opt = sampled_img_ptr_types.iter()
                .find(|(_idx, _, ptr)| ptr.result_id == var.result_type_id);

            let sampled_img_type_opt = ptr_opt.and_then(|(_idx, _, ptr)|
                sampled_img_types.iter()
                    .find(|(_idx, _, img)| img.result_id == ptr.type_id));

            if let Some((_idx, _, sampled_image_type)) = sampled_img_type_opt {
                /*assert_eq!(operand_words[0], var.result_type_id);
                operand_words[0] = sampled_image_type.image_type_id;*/

                mappings.push(SampledImageTypeMappings {
                    image_type: sampled_image_type.image_type_id,
                    image_type_ptr_type: 0,
                    image_type_ptr_var: 0,
                    sampled_image_type: sampled_image_type.result_id,
                    sampled_image_type_ptr_type: ptr_opt.unwrap().2.result_id,
                    sampled_image_type_ptr_var: var.result_id,
                    sampler_type: 0,
                    sampler_type_ptr_type: 0,
                    sampler_type_ptr_var: 0,
                });

                sampled_img_ptr_vars.push((word_index, instruction.clone(), var));
            }
        }
        if instruction.opcode == OP_CODE_OP_LOAD {
            let load = parse_op_load(operand_words);
            if let Some((_idx,_, _)) = sampled_img_ptr_types.iter()
                .find(|(_idx, _, type_id)| type_id.type_id == load.result_type_id) {
                sampled_img_ptr_var_loads.push((word_index, instruction.clone(), load));
            }
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

        true
    });

    let mut insertions = Vec::<(usize, Vec<u32>)>::new();
    for mapping in &mut mappings {
        let words = cast_to_words(spirv);

        // STEP 2: Change type of existing sampled image ptr, ptr var and load to image

        for (ptr_idx, _, ptr) in &sampled_img_ptr_types {
            if ptr.result_id == mapping.sampled_image_type_ptr_type {
                assert_eq!(words[ptr_idx + 3], ptr.type_id);
                assert_ne!(mapping.image_type, 0);
                words[ptr_idx + 3] = mapping.image_type;
            }
        }

        for (load_idx, _, load) in &sampled_img_ptr_var_loads {
            if load.pointer_id != mapping.sampled_image_type_ptr_var {
                continue;
            }
            assert_eq!(load.result_type_id, words[load_idx + 1]);
            words[load_idx + 1] = mapping.image_type;
        }

        mapping.image_type_ptr_type = std::mem::take(&mut mapping.sampled_image_type_ptr_type);
        mapping.image_type_ptr_var = std::mem::take(&mut mapping.sampled_image_type_ptr_var);

        // STEP 3: Insert sampler, sampler ptr, sampler ptr var
        let (var_idx, _, _) = sampled_img_ptr_vars.iter().find(|(_, _, var)| var.result_id == mapping.image_type_ptr_var).unwrap();

        mapping.sampler_type = next_id;
        next_id += 1;
        mapping.sampler_type_ptr_type = next_id;
        next_id += 1;
        mapping.sampler_type_ptr_var = next_id;
        next_id += 1;

        insertions.push((*var_idx, vec![
            build_instruction_description(&Instruction { word_count: 2, opcode: OP_CODE_OP_TYPE_SAMPLER }),
            mapping.sampler_type,
            build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_TYPE_POINTER }),
            mapping.sampler_type_ptr_type,
            STORAGE_CLASS_UNIFORM_CONSTANT,
            mapping.sampler_type,
            build_instruction_description(&Instruction { word_count: 4, opcode: OP_CODE_OP_TYPE_VARIABLE }),
            mapping.sampler_type_ptr_type,
            mapping.sampler_type_ptr_var,
            STORAGE_CLASS_UNIFORM_CONSTANT
        ]));

        // STEP 4: Load sampler and create sampled image

        for (load_idx, instruction, load) in &sampled_img_ptr_var_loads {
            if load.pointer_id != mapping.image_type_ptr_var {
                continue;
            }
            let loaded_image_id = load.result_id;
            let loaded_sampler_id = next_id;
            next_id += 1;
            mapping.sampled_image_type_ptr_var = next_id;
            next_id += 1;

            insertions.push((load_idx + (instruction.word_count as usize), vec![
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

            // STEP 5: Change sampled image id from sampling ids

            for idx in &sampling_ops_indices {
                if words[idx + 3] == loaded_image_id {
                    words[idx + 3] = mapping.sampled_image_type_ptr_var;
                }
            }
        }

        // STEP 6: Add sampler to entry point

        for entry_point_idx in &entry_point_indices {
            let mut entry_point_instruction = parse_instruction_description(words[*entry_point_idx]);
            for word in &words[entry_point_idx + 3 .. entry_point_idx + entry_point_instruction.word_count as usize] {
                if *word == mapping.image_type_ptr_var {
                    insertions.push((entry_point_idx + entry_point_instruction.word_count as usize, vec![
                        mapping.sampler_type_ptr_var
                    ]));
                    entry_point_instruction.word_count += 1;
                }
            }
            words[*entry_point_idx] = build_instruction_description(&entry_point_instruction);
        }
    }

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

    Vec::new()
}


/*

%16 = OpTypeSampler
%_ptr_UniformConstant_16 = OpTypePointer UniformConstant %16
%samp = OpVariable %_ptr_UniformConstant_16 UniformConstant



*/