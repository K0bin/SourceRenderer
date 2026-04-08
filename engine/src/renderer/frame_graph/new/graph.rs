use smallvec::SmallVec;

struct RenderPass {
    resources: SmallVec<[(&'static str, usize); 4]>,
}

pub struct RenderGraph {
    passes: SmallVec<[RenderPass; 8]>
}

impl RenderGraph {

}