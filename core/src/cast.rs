use std::sync::Arc;

pub unsafe fn unsafe_arc_cast<A, B>(item: Arc<A>) -> Arc<B>
  where A: ?Sized {
  let ptr = Arc::into_raw(item);
  let new_ptr = ptr as *const B;
  return Arc::from_raw(new_ptr);
}

pub unsafe fn unsafe_box_cast<A, B>(item: Box<A>) -> Box<B>
  where A: ?Sized {
  let ptr = Box::into_raw(item);
  let new_ptr = ptr as *mut B;
  return Box::from_raw(new_ptr);
}

pub unsafe fn unsafe_ref_cast<A, B>(item: &A) -> &B
  where A: ?Sized {
  let ptr: *const A = item;
  let new_ptr = ptr as *const B;
  return new_ptr.as_ref().unwrap();
}

pub unsafe fn unsafe_mut_cast<A, B>(item: &mut A) -> &mut B
  where A: ?Sized {
  let ptr: *mut A = item;
  let new_ptr = ptr as *mut B;
  return new_ptr.as_mut().unwrap();
}
