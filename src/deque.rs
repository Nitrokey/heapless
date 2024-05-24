use core::fmt;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::{ptr, slice};

pub trait DequeBuffer {
    type T;
    fn as_view(deque: &DequeInner<Self>) -> &DequeView<Self::T>;
    fn as_mut_view(deque: &mut DequeInner<Self>) -> &mut DequeView<Self::T>;
}

impl<T, const N: usize> DequeBuffer for [MaybeUninit<T>; N] {
    type T = T;
    fn as_view(deque: &DequeInner<Self>) -> &DequeView<Self::T> {
        deque
    }
    fn as_mut_view(deque: &mut DequeInner<Self>) -> &mut DequeView<Self::T> {
        deque
    }
}

impl<T> DequeBuffer for [MaybeUninit<T>] {
    type T = T;
    fn as_view(deque: &DequeInner<Self>) -> &DequeView<Self::T> {
        deque
    }
    fn as_mut_view(deque: &mut DequeInner<Self>) -> &mut DequeView<Self::T> {
        deque
    }
}

/// <div class="warn">This is private API and should not be used</div>
pub struct DequeInner<B: ?Sized + DequeBuffer> {
    /// Front index. Always 0..=(N-1)
    front: usize,
    /// Back index. Always 0..=(N-1).
    back: usize,

    /// Used to distinguish "empty" and "full" cases when `front == back`.
    /// May only be `true` if `front == back`, always `false` otherwise.
    full: bool,

    buffer: B,
}

/// A fixed capacity double-ended queue.
///
/// # Examples
///
/// ```
/// use heapless::Deque;
///
/// // A deque with a fixed capacity of 8 elements allocated on the stack
/// let mut deque = Deque::<_, 8>::new();
///
/// // You can use it as a good old FIFO queue.
/// deque.push_back(1);
/// deque.push_back(2);
/// assert_eq!(deque.len(), 2);
///
/// assert_eq!(deque.pop_front(), Some(1));
/// assert_eq!(deque.pop_front(), Some(2));
/// assert_eq!(deque.len(), 0);
///
/// // Deque is double-ended, you can push and pop from the front and back.
/// deque.push_back(1);
/// deque.push_front(2);
/// deque.push_back(3);
/// deque.push_front(4);
/// assert_eq!(deque.pop_front(), Some(4));
/// assert_eq!(deque.pop_front(), Some(2));
/// assert_eq!(deque.pop_front(), Some(1));
/// assert_eq!(deque.pop_front(), Some(3));
///
/// // You can iterate it, yielding all the elements front-to-back.
/// for x in &deque {
///     println!("{}", x);
/// }
/// ```
pub type Deque<T, const N: usize> = DequeInner<[MaybeUninit<T>; N]>;

/// A double-ended queue with dynamic capacity.
///
/// # Examples
///
/// ```
/// use heapless::{Deque, DequeView};
///
/// // A deque with a fixed capacity of 8 elements allocated on the stack
/// let mut deque_buf = Deque::<_, 8>::new();
///
/// // A DequeView can be obtained through unsized coercion of a `Deque`
/// let deque: &mut DequeView<_> = &mut deque_buf;
///
/// // You can use it as a good old FIFO queue.
/// deque.push_back(1);
/// deque.push_back(2);
/// assert_eq!(deque.len(), 2);
///
/// assert_eq!(deque.pop_front(), Some(1));
/// assert_eq!(deque.pop_front(), Some(2));
/// assert_eq!(deque.len(), 0);
///
/// // DequeView is double-ended, you can push and pop from the front and back.
/// deque.push_back(1);
/// deque.push_front(2);
/// deque.push_back(3);
/// deque.push_front(4);
/// assert_eq!(deque.pop_front(), Some(4));
/// assert_eq!(deque.pop_front(), Some(2));
/// assert_eq!(deque.pop_front(), Some(1));
/// assert_eq!(deque.pop_front(), Some(3));
///
/// // You can iterate it, yielding all the elements front-to-back.
/// for x in deque {
///     println!("{}", x);
/// }
/// ```
pub type DequeView<T> = DequeInner<[MaybeUninit<T>]>;

impl<T> DequeView<T> {
    fn increment(&self, i: usize) -> usize {
        if i + 1 == self.capacity() {
            0
        } else {
            i + 1
        }
    }

    fn decrement(&self, i: usize) -> usize {
        if i == 0 {
            self.capacity() - 1
        } else {
            i - 1
        }
    }

    /// Returns the maximum number of elements the deque can hold.
    pub const fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// Returns the number of elements currently in the deque.
    pub const fn len(&self) -> usize {
        if self.full {
            self.capacity()
        } else if self.back < self.front {
            self.back + self.capacity() - self.front
        } else {
            self.back - self.front
        }
    }

    /// Clears the deque, removing all values.
    pub fn clear(&mut self) {
        // safety: we're immediately setting a consistent empty state.
        unsafe { self.drop_contents() }
        self.front = 0;
        self.back = 0;
        self.full = false;
    }

    /// Drop all items in the `Deque`, leaving the state `back/front/full` unmodified.
    ///                            
    /// safety: leaves the `Deque` in an inconsistent state, so can cause duplicate drops.
    unsafe fn drop_contents(&mut self) {
        // We drop each element used in the deque by turning into a &mut[T]
        let (a, b) = self.as_mut_slices();
        ptr::drop_in_place(a);
        ptr::drop_in_place(b);
    }
    /// Returns whether the deque is empty.
    pub fn is_empty(&self) -> bool {
        self.front == self.back && !self.full
    }

    /// Returns whether the deque is full (i.e. if `len() == capacity()`.
    pub fn is_full(&self) -> bool {
        self.full
    }

    /// Returns a pair of slices which contain, in order, the contents of the `Deque`.
    pub fn as_slices(&self) -> (&[T], &[T]) {
        // NOTE(unsafe) avoid bound checks in the slicing operation
        unsafe {
            if self.is_empty() {
                (&[], &[])
            } else if self.back <= self.front {
                (
                    slice::from_raw_parts(
                        self.buffer.as_ptr().add(self.front) as *const T,
                        self.capacity() - self.front,
                    ),
                    slice::from_raw_parts(self.buffer.as_ptr() as *const T, self.back),
                )
            } else {
                (
                    slice::from_raw_parts(
                        self.buffer.as_ptr().add(self.front) as *const T,
                        self.back - self.front,
                    ),
                    &[],
                )
            }
        }
    }

    /// Returns a pair of mutable slices which contain, in order, the contents of the `Deque`.
    pub fn as_mut_slices(&mut self) -> (&mut [T], &mut [T]) {
        let ptr = self.buffer.as_mut_ptr();
        let is_empty = self.front == self.back && !self.full;

        // NOTE(unsafe) avoid bound checks in the slicing operation
        unsafe {
            if is_empty {
                (&mut [], &mut [])
            } else if self.back <= self.front {
                (
                    slice::from_raw_parts_mut(
                        ptr.add(self.front) as *mut T,
                        self.buffer.len() - self.front,
                    ),
                    slice::from_raw_parts_mut(ptr as *mut T, self.back),
                )
            } else {
                (
                    slice::from_raw_parts_mut(
                        ptr.add(self.front) as *mut T,
                        self.back - self.front,
                    ),
                    &mut [],
                )
            }
        }
    }

    #[inline]
    fn is_contiguous(&self) -> bool {
        self.front <= self.capacity() - self.len()
    }

    /// Rearranges the internal storage of the [`Deque`] to make it into a contiguous slice,
    /// which is returned.
    ///
    /// This does **not** change the order of the elements in the deque.
    /// The returned slice can then be used to perform contiguous slice operations on the deque.
    ///
    /// After calling this method, subsequent [`as_slices`] and [`as_mut_slices`] calls will return
    /// a single contiguous slice.
    ///
    /// [`as_slices`]: Deque::as_slices
    /// [`as_mut_slices`]: Deque::as_mut_slices
    ///
    /// # Examples
    /// Sorting a deque:
    /// ```
    /// use heapless::{Deque, DequeView};
    ///
    /// let mut deque_buf = Deque::<_, 4>::new();
    /// let buf: &mut DequeView<_> = &mut deque_buf;
    /// buf.push_back(2).unwrap();
    /// buf.push_back(1).unwrap();
    /// buf.push_back(3).unwrap();
    ///
    /// // Sort the deque
    /// buf.make_contiguous().sort();
    /// assert_eq!(buf.as_slices(), (&[1, 2, 3][..], &[][..]));
    ///
    /// // Sort the deque in reverse
    /// buf.make_contiguous().sort_by(|a, b| b.cmp(a));
    /// assert_eq!(buf.as_slices(), (&[3, 2, 1][..], &[][..]));
    /// ```
    pub fn make_contiguous(&mut self) -> &mut [T] {
        if self.is_contiguous() {
            return unsafe {
                slice::from_raw_parts_mut(
                    self.buffer.as_mut_ptr().add(self.front).cast(),
                    self.len(),
                )
            };
        }

        let buffer_ptr: *mut T = self.buffer.as_mut_ptr().cast();

        let len = self.len();

        let free = self.capacity() - len;
        let front_len = self.capacity() - self.front;
        let back = len - front_len;
        let back_len = back;

        if free >= front_len {
            // there is enough free space to copy the head in one go,
            // this means that we first shift the tail backwards, and then
            // copy the head to the correct position.
            //
            // from: DEFGH....ABC
            // to:   ABCDEFGH....
            unsafe {
                ptr::copy(buffer_ptr, buffer_ptr.add(front_len), back_len);
                // ...DEFGH.ABC
                ptr::copy_nonoverlapping(buffer_ptr.add(self.front), buffer_ptr, front_len);
                // ABCDEFGH....
            }

            self.front = 0;
            self.back = len;
        } else if free >= back_len {
            // there is enough free space to copy the tail in one go,
            // this means that we first shift the head forwards, and then
            // copy the tail to the correct position.
            //
            // from: FGH....ABCDE
            // to:   ...ABCDEFGH.
            unsafe {
                ptr::copy(
                    buffer_ptr.add(self.front),
                    buffer_ptr.add(self.back),
                    front_len,
                );
                // FGHABCDE....
                ptr::copy_nonoverlapping(
                    buffer_ptr,
                    buffer_ptr.add(self.back + front_len),
                    back_len,
                );
                // ...ABCDEFGH.
            }

            self.front = back;
            self.back = 0;
        } else {
            // `free` is smaller than both `head_len` and `tail_len`.
            // the general algorithm for this first moves the slices
            // right next to each other and then uses `slice::rotate`
            // to rotate them into place:
            //
            // initially:   HIJK..ABCDEFG
            // step 1:      ..HIJKABCDEFG
            // step 2:      ..ABCDEFGHIJK
            //
            // or:
            //
            // initially:   FGHIJK..ABCDE
            // step 1:      FGHIJKABCDE..
            // step 2:      ABCDEFGHIJK..

            // pick the shorter of the 2 slices to reduce the amount
            // of memory that needs to be moved around.
            if front_len > back_len {
                // tail is shorter, so:
                //  1. copy tail forwards
                //  2. rotate used part of the buffer
                //  3. update head to point to the new beginning (which is just `free`)
                unsafe {
                    // if there is no free space in the buffer, then the slices are already
                    // right next to each other and we don't need to move any memory.
                    if free != 0 {
                        // because we only move the tail forward as much as there's free space
                        // behind it, we don't overwrite any elements of the head slice, and
                        // the slices end up right next to each other.
                        ptr::copy(buffer_ptr, buffer_ptr.add(free), back_len);
                    }

                    // We just copied the tail right next to the head slice,
                    // so all of the elements in the range are initialized
                    let slice: &mut [T] =
                        slice::from_raw_parts_mut(buffer_ptr.add(free), self.capacity() - free);

                    // because the deque wasn't contiguous, we know that `tail_len < self.len == slice.len()`,
                    // so this will never panic.
                    slice.rotate_left(back_len);

                    // the used part of the buffer now is `free..self.capacity()`, so set
                    // `head` to the beginning of that range.
                    self.front = free;
                    self.back = 0;
                }
            } else {
                // head is shorter so:
                //  1. copy head backwards
                //  2. rotate used part of the buffer
                //  3. update head to point to the new beginning (which is the beginning of the buffer)

                unsafe {
                    // if there is no free space in the buffer, then the slices are already
                    // right next to each other and we don't need to move any memory.
                    if free != 0 {
                        // copy the head slice to lie right behind the tail slice.
                        ptr::copy(
                            buffer_ptr.add(self.front),
                            buffer_ptr.add(back_len),
                            front_len,
                        );
                    }

                    // because we copied the head slice so that both slices lie right
                    // next to each other, all the elements in the range are initialized.
                    let slice: &mut [T] = slice::from_raw_parts_mut(buffer_ptr, len);

                    // because the deque wasn't contiguous, we know that `head_len < self.len == slice.len()`
                    // so this will never panic.
                    slice.rotate_right(front_len);

                    // the used part of the buffer now is `0..self.len`, so set
                    // `head` to the beginning of that range.
                    self.front = 0;
                    self.back = len;
                }
            }
        }

        unsafe { slice::from_raw_parts_mut(buffer_ptr.add(self.front), len) }
    }

    /// Provides a reference to the front element, or None if the `Deque` is empty.
    pub fn front(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            Some(unsafe { &*self.buffer.get_unchecked(self.front).as_ptr() })
        }
    }

    /// Provides a mutable reference to the front element, or None if the `Deque` is empty.
    pub fn front_mut(&mut self) -> Option<&mut T> {
        if self.is_empty() {
            None
        } else {
            Some(unsafe { &mut *self.buffer.get_unchecked_mut(self.front).as_mut_ptr() })
        }
    }

    /// Provides a reference to the back element, or None if the `Deque` is empty.
    pub fn back(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            let index = self.decrement(self.back);
            Some(unsafe { &*self.buffer.get_unchecked(index).as_ptr() })
        }
    }

    /// Provides a mutable reference to the back element, or None if the `Deque` is empty.
    pub fn back_mut(&mut self) -> Option<&mut T> {
        if self.is_empty() {
            None
        } else {
            let index = self.decrement(self.back);
            Some(unsafe { &mut *self.buffer.get_unchecked_mut(index).as_mut_ptr() })
        }
    }

    /// Removes the item from the front of the deque and returns it, or `None` if it's empty
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            Some(unsafe { self.pop_front_unchecked() })
        }
    }

    /// Removes the item from the back of the deque and returns it, or `None` if it's empty
    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            Some(unsafe { self.pop_back_unchecked() })
        }
    }

    /// Appends an `item` to the front of the deque
    ///
    /// Returns back the `item` if the deque is full
    pub fn push_front(&mut self, item: T) -> Result<(), T> {
        if self.is_full() {
            Err(item)
        } else {
            unsafe { self.push_front_unchecked(item) }
            Ok(())
        }
    }

    /// Appends an `item` to the back of the deque
    ///
    /// Returns back the `item` if the deque is full
    pub fn push_back(&mut self, item: T) -> Result<(), T> {
        if self.is_full() {
            Err(item)
        } else {
            unsafe { self.push_back_unchecked(item) }
            Ok(())
        }
    }

    /// Removes an item from the front of the deque and returns it, without checking that the deque
    /// is not empty
    ///
    /// # Safety
    ///
    /// It's undefined behavior to call this on an empty deque
    pub unsafe fn pop_front_unchecked(&mut self) -> T {
        debug_assert!(!self.is_empty());

        let index = self.front;
        self.full = false;
        self.front = self.increment(self.front);
        self.buffer.get_unchecked_mut(index).as_ptr().read()
    }

    /// Removes an item from the back of the deque and returns it, without checking that the deque
    /// is not empty
    ///
    /// # Safety
    ///
    /// It's undefined behavior to call this on an empty deque
    pub unsafe fn pop_back_unchecked(&mut self) -> T {
        debug_assert!(!self.is_empty());

        self.full = false;
        self.back = self.decrement(self.back);
        self.buffer.get_unchecked_mut(self.back).as_ptr().read()
    }

    /// Appends an `item` to the front of the deque
    ///
    /// # Safety
    ///
    /// This assumes the deque is not full.
    pub unsafe fn push_front_unchecked(&mut self, item: T) {
        debug_assert!(!self.is_full());

        let index = self.decrement(self.front);
        // NOTE: the memory slot that we are about to write to is uninitialized. We assign
        // a `MaybeUninit` to avoid running `T`'s destructor on the uninitialized memory
        *self.buffer.get_unchecked_mut(index) = MaybeUninit::new(item);
        self.front = index;
        if self.front == self.back {
            self.full = true;
        }
    }

    /// Appends an `item` to the back of the deque
    ///
    /// # Safety
    ///
    /// This assumes the deque is not full.
    pub unsafe fn push_back_unchecked(&mut self, item: T) {
        debug_assert!(!self.is_full());

        // NOTE: the memory slot that we are about to write to is uninitialized. We assign
        // a `MaybeUninit` to avoid running `T`'s destructor on the uninitialized memory
        *self.buffer.get_unchecked_mut(self.back) = MaybeUninit::new(item);
        self.back = self.increment(self.back);
        if self.front == self.back {
            self.full = true;
        }
    }

    /// Returns an iterator over the deque.
    pub fn iter(&self) -> IterView<'_, T> {
        let (a, b) = self.as_slices();

        IterView {
            inner: a.iter().chain(b),
        }
    }

    /// Returns an iterator that allows modifying each value.
    pub fn iter_mut(&mut self) -> IterViewMut<'_, T> {
        let (a, b) = self.as_mut_slices();

        IterViewMut {
            inner: a.iter_mut().chain(b),
        }
    }
}

impl<T, const N: usize> Deque<T, N> {
    const INIT: MaybeUninit<T> = MaybeUninit::uninit();

    /// Constructs a new, empty deque with a fixed capacity of `N`
    ///
    /// # Examples
    ///
    /// ```
    /// use heapless::Deque;
    ///
    /// // allocate the deque on the stack
    /// let mut x: Deque<u8, 16> = Deque::new();
    ///
    /// // allocate the deque in a static variable
    /// static mut X: Deque<u8, 16> = Deque::new();
    /// ```
    pub const fn new() -> Self {
        // Const assert N > 0
        crate::sealed::greater_than_0::<N>();

        Self {
            buffer: [Self::INIT; N],
            front: 0,
            back: 0,
            full: false,
        }
    }

    /// Get a reference to the `Deque`, erasing the `N` const-generic.
    ///
    /// ```rust
    /// # use heapless::{Deque, DequeView};
    /// let deque: Deque<u8, 10> = Deque::new();
    /// let view: &DequeView<u8> = deque.as_view();
    /// ```
    ///
    /// It is often preferable to do the same through type coerction, since `Deque<T, N>` implements `Unsize<DequeView<T>>`:
    ///
    /// ```rust
    /// # use heapless::{Deque, DequeView};
    /// let deque: Deque<u8, 10> = Deque::new();
    /// let view: &DequeView<u8> = &deque;
    /// ```
    pub const fn as_view(&self) -> &DequeView<T> {
        self
    }

    /// Get a mutable reference to the `Deque`, erasing the `N` const-generic.
    ///
    /// ```rust
    /// # use heapless::{Deque, DequeView};
    /// let mut deque: Deque<u8, 10> = Deque::new();
    /// let view: &mut DequeView<u8> = deque.as_mut_view();
    /// ```
    ///
    /// It is often preferable to do the same through type coerction, since `Deque<T, N>` implements `Unsize<DequeView<T>>`:
    ///
    /// ```rust
    /// # use heapless::{Deque, DequeView};
    /// let mut deque: Deque<u8, 10> = Deque::new();
    /// let view: &mut DequeView<u8> = &mut deque;
    /// ```
    pub fn as_mut_view(&mut self) -> &mut DequeView<T> {
        self
    }

    fn increment(i: usize) -> usize {
        if i + 1 == N {
            0
        } else {
            i + 1
        }
    }

    fn decrement(i: usize) -> usize {
        if i == 0 {
            N - 1
        } else {
            i - 1
        }
    }

    /// Returns the maximum number of elements the deque can hold.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Returns the number of elements currently in the deque.
    #[inline]
    pub const fn len(&self) -> usize {
        self.as_view().len()
    }

    /// Clears the deque, removing all values.
    #[inline]
    pub fn clear(&mut self) {
        self.as_mut_view().clear()
    }

    /// Returns whether the deque is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.as_view().is_empty()
    }

    /// Returns whether the deque is full (i.e. if `len() == capacity()`.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.as_view().is_full()
    }

    /// Returns a pair of slices which contain, in order, the contents of the `Deque`.
    #[inline]
    pub fn as_slices(&self) -> (&[T], &[T]) {
        self.as_view().as_slices()
    }

    /// Returns a pair of mutable slices which contain, in order, the contents of the `Deque`.
    #[inline]
    pub fn as_mut_slices(&mut self) -> (&mut [T], &mut [T]) {
        self.as_mut_view().as_mut_slices()
    }

    /// Rearranges the internal storage of the [`Deque`] to make it into a contiguous slice,
    /// which is returned.
    ///
    /// This does **not** change the order of the elements in the deque.
    /// The returned slice can then be used to perform contiguous slice operations on the deque.
    ///
    /// After calling this method, subsequent [`as_slices`] and [`as_mut_slices`] calls will return
    /// a single contiguous slice.
    ///
    /// [`as_slices`]: Deque::as_slices
    /// [`as_mut_slices`]: Deque::as_mut_slices
    ///
    /// # Examples
    /// Sorting a deque:
    /// ```
    /// use heapless::Deque;
    ///
    /// let mut buf = Deque::<_, 4>::new();
    /// buf.push_back(2).unwrap();
    /// buf.push_back(1).unwrap();
    /// buf.push_back(3).unwrap();
    ///
    /// // Sort the deque
    /// buf.make_contiguous().sort();
    /// assert_eq!(buf.as_slices(), (&[1, 2, 3][..], &[][..]));
    ///
    /// // Sort the deque in reverse
    /// buf.make_contiguous().sort_by(|a, b| b.cmp(a));
    /// assert_eq!(buf.as_slices(), (&[3, 2, 1][..], &[][..]));
    /// ```
    #[inline]
    pub fn make_contiguous(&mut self) -> &mut [T] {
        self.as_mut_view().make_contiguous()
    }

    /// Provides a reference to the front element, or None if the `Deque` is empty.
    #[inline]
    pub fn front(&self) -> Option<&T> {
        self.as_view().front()
    }

    /// Provides a mutable reference to the front element, or None if the `Deque` is empty.
    #[inline]
    pub fn front_mut(&mut self) -> Option<&mut T> {
        self.as_mut_view().front_mut()
    }

    /// Provides a reference to the back element, or None if the `Deque` is empty.
    #[inline]
    pub fn back(&self) -> Option<&T> {
        self.as_view().back()
    }

    /// Provides a mutable reference to the back element, or None if the `Deque` is empty.
    #[inline]
    pub fn back_mut(&mut self) -> Option<&mut T> {
        self.as_mut_view().back_mut()
    }

    /// Removes the item from the front of the deque and returns it, or `None` if it's empty
    #[inline]
    pub fn pop_front(&mut self) -> Option<T> {
        self.as_mut_view().pop_front()
    }

    /// Removes the item from the back of the deque and returns it, or `None` if it's empty
    #[inline]
    pub fn pop_back(&mut self) -> Option<T> {
        self.as_mut_view().pop_back()
    }

    /// Appends an `item` to the front of the deque
    ///
    /// Returns back the `item` if the deque is full
    #[inline]
    pub fn push_front(&mut self, item: T) -> Result<(), T> {
        self.as_mut_view().push_front(item)
    }

    /// Appends an `item` to the back of the deque
    ///
    /// Returns back the `item` if the deque is full
    #[inline]
    pub fn push_back(&mut self, item: T) -> Result<(), T> {
        self.as_mut_view().push_back(item)
    }

    /// Removes an item from the front of the deque and returns it, without checking that the deque
    /// is not empty
    ///
    /// # Safety
    ///
    /// It's undefined behavior to call this on an empty deque
    #[inline]
    pub unsafe fn pop_front_unchecked(&mut self) -> T {
        self.as_mut_view().pop_front_unchecked()
    }

    /// Removes an item from the back of the deque and returns it, without checking that the deque
    /// is not empty
    ///
    /// # Safety
    ///
    /// It's undefined behavior to call this on an empty deque
    #[inline]
    pub unsafe fn pop_back_unchecked(&mut self) -> T {
        self.as_mut_view().pop_back_unchecked()
    }

    /// Appends an `item` to the front of the deque
    ///
    /// # Safety
    ///
    /// This assumes the deque is not full.
    #[inline]
    pub unsafe fn push_front_unchecked(&mut self, item: T) {
        self.as_mut_view().push_front_unchecked(item)
    }

    /// Appends an `item` to the back of the deque
    ///
    /// # Safety
    ///
    /// This assumes the deque is not full.
    #[inline]
    pub unsafe fn push_back_unchecked(&mut self, item: T) {
        self.as_mut_view().push_back_unchecked(item)
    }

    /// Returns an iterator over the deque.
    pub fn iter(&self) -> Iter<'_, T, N> {
        let done = self.is_empty();
        Iter {
            _phantom: PhantomData,
            buffer: &self.buffer as *const MaybeUninit<T>,
            front: self.front,
            back: self.back,
            done,
        }
    }

    /// Returns an iterator that allows modifying each value.
    pub fn iter_mut(&mut self) -> IterMut<'_, T, N> {
        let done = self.is_empty();
        IterMut {
            _phantom: PhantomData,
            buffer: &mut self.buffer as *mut _ as *mut MaybeUninit<T>,
            front: self.front,
            back: self.back,
            done,
        }
    }
}

// Trait implementations

impl<T, const N: usize> Default for Deque<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: ?Sized + DequeBuffer> Drop for DequeInner<B> {
    fn drop(&mut self) {
        let (a, b) = DequeBuffer::as_mut_view(self).as_mut_slices();
        // SAFETY: The slices of the deque contain all the initialized data of the deque.
        unsafe {
            ptr::drop_in_place(a);
            ptr::drop_in_place(b);
        }
    }
}

macro_rules! imp_traits {
    ($Ty:ident$(<const $N:ident : usize, const $M:ident : usize>)?) => {
        impl<T, $(const $M: usize)*> fmt::Debug for $Ty<T, $($M)*>
        where T: fmt::Debug
        {
            #[inline]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_list().entries(self).finish()
            }
        }

        /// As with the standard library's `VecDeque`, items are added via `push_back`.
        impl<T, $(const $M: usize)*> Extend<T> for $Ty<T, $($M)*>
        {
            fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
                for item in iter {
                    self.push_back(item).ok().unwrap();
                }
            }
        }


        impl<'a, T: 'a + Copy, $(const $M: usize)*> Extend<&'a T> for $Ty<T, $($M)*> {
            fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
                self.extend(iter.into_iter().copied())
            }
        }
    }
}

imp_traits!(Deque<const N: usize, const M: usize>);
imp_traits!(DequeView);

/// An iterator that moves out of a [`Deque`].
///
/// This struct is created by calling the `into_iter` method.
#[derive(Clone)]
pub struct IntoIter<T, const N: usize> {
    deque: Deque<T, N>,
}

impl<T, const N: usize> Iterator for IntoIter<T, N> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.deque.pop_front()
    }
}

impl<T, const N: usize> IntoIterator for Deque<T, N> {
    type Item = T;
    type IntoIter = IntoIter<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter { deque: self }
    }
}

/// An iterator over the elements of a [`Deque`].
///
/// This struct is created by calling the `iter` method.
#[derive(Clone)]
pub struct Iter<'a, T, const N: usize> {
    buffer: *const MaybeUninit<T>,
    _phantom: PhantomData<&'a T>,
    front: usize,
    back: usize,
    done: bool,
}

impl<'a, T, const N: usize> Iterator for Iter<'a, T, N> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            None
        } else {
            let index = self.front;
            self.front = Deque::<T, N>::increment(self.front);
            if self.front == self.back {
                self.done = true;
            }
            Some(unsafe { &*(self.buffer.add(index) as *const T) })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = if self.done {
            0
        } else if self.back <= self.front {
            self.back + N - self.front
        } else {
            self.back - self.front
        };

        (len, Some(len))
    }
}

impl<'a, T, const N: usize> DoubleEndedIterator for Iter<'a, T, N> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.done {
            None
        } else {
            self.back = Deque::<T, N>::decrement(self.back);
            if self.front == self.back {
                self.done = true;
            }
            Some(unsafe { &*(self.buffer.add(self.back) as *const T) })
        }
    }
}

impl<'a, T, const N: usize> ExactSizeIterator for Iter<'a, T, N> {}
impl<'a, T, const N: usize> FusedIterator for Iter<'a, T, N> {}

/// An iterator over the elements of a [`Deque`].
///
/// This struct is created by calling the `iter` method.
pub struct IterMut<'a, T, const N: usize> {
    buffer: *mut MaybeUninit<T>,
    _phantom: PhantomData<&'a mut T>,
    front: usize,
    back: usize,
    done: bool,
}

impl<'a, T, const N: usize> Iterator for IterMut<'a, T, N> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            None
        } else {
            let index = self.front;
            self.front = Deque::<T, N>::increment(self.front);
            if self.front == self.back {
                self.done = true;
            }
            Some(unsafe { &mut *(self.buffer.add(index) as *mut T) })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = if self.done {
            0
        } else if self.back <= self.front {
            self.back + N - self.front
        } else {
            self.back - self.front
        };

        (len, Some(len))
    }
}

impl<'a, T, const N: usize> DoubleEndedIterator for IterMut<'a, T, N> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.done {
            None
        } else {
            self.back = Deque::<T, N>::decrement(self.back);
            if self.front == self.back {
                self.done = true;
            }
            Some(unsafe { &mut *(self.buffer.add(self.back) as *mut T) })
        }
    }
}

impl<'a, T, const N: usize> ExactSizeIterator for IterMut<'a, T, N> {}
impl<'a, T, const N: usize> FusedIterator for IterMut<'a, T, N> {}

impl<'a, T, const N: usize> IntoIterator for &'a Deque<T, N> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T, N>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut Deque<T, N> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T, N>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

pub struct IterView<'a, T> {
    inner: core::iter::Chain<core::slice::Iter<'a, T>, core::slice::Iter<'a, T>>,
}

impl<'a, T> Iterator for IterView<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, T> DoubleEndedIterator for IterView<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'a, T> ExactSizeIterator for IterView<'a, T> {}
impl<'a, T> FusedIterator for IterView<'a, T> {}

pub struct IterViewMut<'a, T> {
    inner: core::iter::Chain<core::slice::IterMut<'a, T>, core::slice::IterMut<'a, T>>,
}

impl<'a, T> Iterator for IterViewMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, T> DoubleEndedIterator for IterViewMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'a, T> ExactSizeIterator for IterViewMut<'a, T> {}
impl<'a, T> FusedIterator for IterViewMut<'a, T> {}

impl<'a, T> IntoIterator for &'a DequeView<T> {
    type Item = &'a T;
    type IntoIter = IterView<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut DequeView<T> {
    type Item = &'a mut T;
    type IntoIter = IterViewMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T, const N: usize> Clone for Deque<T, N>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        let mut res = Deque::new();
        for i in self {
            // safety: the original and new deques have the same capacity, so it can
            // not become full.
            unsafe { res.push_back_unchecked(i.clone()) }
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use static_assertions::assert_not_impl_any;

    use super::{Deque, DequeView};

    // Ensure a `Deque` containing `!Send` values stays `!Send` itself.
    assert_not_impl_any!(Deque<*const (), 4>: Send);

    #[test]
    fn static_new() {
        static mut _V: Deque<i32, 4> = Deque::new();
    }

    #[test]
    fn stack_new() {
        let mut _v: Deque<i32, 4> = Deque::new();
    }

    #[test]
    fn drop() {
        droppable!();

        {
            let mut v: Deque<Droppable, 2> = Deque::new();
            v.push_back(Droppable::new()).ok().unwrap();
            v.push_back(Droppable::new()).ok().unwrap();
            v.pop_front().unwrap();
        }

        assert_eq!(Droppable::count(), 0);

        {
            let mut v: Deque<Droppable, 2> = Deque::new();
            v.push_back(Droppable::new()).ok().unwrap();
            v.push_back(Droppable::new()).ok().unwrap();
        }

        assert_eq!(Droppable::count(), 0);
        {
            let mut v: Deque<Droppable, 2> = Deque::new();
            v.push_front(Droppable::new()).ok().unwrap();
            v.push_front(Droppable::new()).ok().unwrap();
        }

        assert_eq!(Droppable::count(), 0);
    }

    #[test]
    fn drop_view() {
        droppable!();

        {
            let v: Deque<Droppable, 2> = Deque::new();
            let mut v: Box<DequeView<Droppable>> = Box::new(v);
            v.push_back(Droppable::new()).ok().unwrap();
            v.push_back(Droppable::new()).ok().unwrap();
            assert_eq!(Droppable::count(), 2);
            v.pop_front().unwrap();
            assert_eq!(Droppable::count(), 1);
        }

        assert_eq!(Droppable::count(), 0);

        {
            let v: Deque<Droppable, 2> = Deque::new();
            let mut v: Box<DequeView<Droppable>> = Box::new(v);
            v.push_back(Droppable::new()).ok().unwrap();
            v.push_back(Droppable::new()).ok().unwrap();
            assert_eq!(Droppable::count(), 2);
        }

        assert_eq!(Droppable::count(), 0);
        {
            let v: Deque<Droppable, 2> = Deque::new();
            let mut v: Box<DequeView<Droppable>> = Box::new(v);
            v.push_front(Droppable::new()).ok().unwrap();
            v.push_front(Droppable::new()).ok().unwrap();
            assert_eq!(Droppable::count(), 2);
        }

        assert_eq!(Droppable::count(), 0);
    }

    #[test]
    fn full() {
        let mut v: Deque<i32, 4> = Deque::new();

        v.push_back(0).unwrap();
        v.push_front(1).unwrap();
        v.push_back(2).unwrap();
        v.push_back(3).unwrap();

        assert!(v.push_front(4).is_err());
        assert!(v.push_back(4).is_err());
        assert!(v.is_full());
    }

    #[test]
    fn empty() {
        let mut v: Deque<i32, 4> = Deque::new();
        assert!(v.is_empty());

        v.push_back(0).unwrap();
        assert!(!v.is_empty());

        v.push_front(1).unwrap();
        assert!(!v.is_empty());

        v.pop_front().unwrap();
        v.pop_front().unwrap();

        assert!(v.pop_front().is_none());
        assert!(v.pop_back().is_none());
        assert!(v.is_empty());
    }

    #[test]
    fn front_back() {
        let mut v: Deque<i32, 4> = Deque::new();
        assert_eq!(v.front(), None);
        assert_eq!(v.front_mut(), None);
        assert_eq!(v.back(), None);
        assert_eq!(v.back_mut(), None);

        v.push_back(4).unwrap();
        assert_eq!(v.front(), Some(&4));
        assert_eq!(v.front_mut(), Some(&mut 4));
        assert_eq!(v.back(), Some(&4));
        assert_eq!(v.back_mut(), Some(&mut 4));

        v.push_front(3).unwrap();
        assert_eq!(v.front(), Some(&3));
        assert_eq!(v.front_mut(), Some(&mut 3));
        assert_eq!(v.back(), Some(&4));
        assert_eq!(v.back_mut(), Some(&mut 4));

        v.pop_back().unwrap();
        assert_eq!(v.front(), Some(&3));
        assert_eq!(v.front_mut(), Some(&mut 3));
        assert_eq!(v.back(), Some(&3));
        assert_eq!(v.back_mut(), Some(&mut 3));

        v.pop_front().unwrap();
        assert_eq!(v.front(), None);
        assert_eq!(v.front_mut(), None);
        assert_eq!(v.back(), None);
        assert_eq!(v.back_mut(), None);
    }

    #[test]
    fn extend() {
        let mut v: Deque<i32, 4> = Deque::new();
        v.extend(&[1, 2, 3]);
        assert_eq!(v.pop_front().unwrap(), 1);
        assert_eq!(v.pop_front().unwrap(), 2);
        assert_eq!(*v.front().unwrap(), 3);

        v.push_back(4).unwrap();
        v.extend(&[5, 6]);
        assert_eq!(v.pop_front().unwrap(), 3);
        assert_eq!(v.pop_front().unwrap(), 4);
        assert_eq!(v.pop_front().unwrap(), 5);
        assert_eq!(v.pop_front().unwrap(), 6);
        assert!(v.pop_front().is_none());
    }

    #[test]
    #[should_panic]
    fn extend_panic() {
        let mut v: Deque<i32, 4> = Deque::new();
        // Is too many elements -> should panic
        v.extend(&[1, 2, 3, 4, 5]);
    }

    #[test]
    fn iter() {
        let mut v: Deque<i32, 4> = Deque::new();

        v.push_back(0).unwrap();
        v.push_back(1).unwrap();
        v.push_front(2).unwrap();
        v.push_front(3).unwrap();
        v.pop_back().unwrap();
        v.push_front(4).unwrap();

        let mut items = v.iter();

        assert_eq!(items.next(), Some(&4));
        assert_eq!(items.next(), Some(&3));
        assert_eq!(items.next(), Some(&2));
        assert_eq!(items.next(), Some(&0));
        assert_eq!(items.next(), None);
    }

    #[test]
    fn iter_mut() {
        let mut v: Deque<i32, 4> = Deque::new();

        v.push_back(0).unwrap();
        v.push_back(1).unwrap();
        v.push_front(2).unwrap();
        v.push_front(3).unwrap();
        v.pop_back().unwrap();
        v.push_front(4).unwrap();

        let mut items = v.iter_mut();

        assert_eq!(items.next(), Some(&mut 4));
        assert_eq!(items.next(), Some(&mut 3));
        assert_eq!(items.next(), Some(&mut 2));
        assert_eq!(items.next(), Some(&mut 0));
        assert_eq!(items.next(), None);
    }

    #[test]
    fn iter_move() {
        let mut v: Deque<i32, 4> = Deque::new();
        v.push_back(0).unwrap();
        v.push_back(1).unwrap();
        v.push_back(2).unwrap();
        v.push_back(3).unwrap();

        let mut items = v.into_iter();

        assert_eq!(items.next(), Some(0));
        assert_eq!(items.next(), Some(1));
        assert_eq!(items.next(), Some(2));
        assert_eq!(items.next(), Some(3));
        assert_eq!(items.next(), None);
    }

    #[test]
    fn iter_move_drop() {
        droppable!();

        {
            let mut deque: Deque<Droppable, 2> = Deque::new();
            deque.push_back(Droppable::new()).ok().unwrap();
            deque.push_back(Droppable::new()).ok().unwrap();
            let mut items = deque.into_iter();
            // Move all
            let _ = items.next().unwrap();
            let _ = items.next().unwrap();
        }

        assert_eq!(Droppable::count(), 0);

        {
            let mut deque: Deque<Droppable, 2> = Deque::new();
            deque.push_back(Droppable::new()).ok().unwrap();
            deque.push_back(Droppable::new()).ok().unwrap();
            let _items = deque.into_iter();
            // Move none
        }

        assert_eq!(Droppable::count(), 0);

        {
            let mut deque: Deque<Droppable, 2> = Deque::new();
            deque.push_back(Droppable::new()).ok().unwrap();
            deque.push_back(Droppable::new()).ok().unwrap();
            let mut items = deque.into_iter();
            let _ = items.next(); // Move partly
        }

        assert_eq!(Droppable::count(), 0);
    }

    #[test]
    fn push_and_pop() {
        let mut q: Deque<i32, 4> = Deque::new();
        assert_eq!(q.len(), 0);

        assert_eq!(q.pop_front(), None);
        assert_eq!(q.pop_back(), None);
        assert_eq!(q.len(), 0);

        q.push_back(0).unwrap();
        assert_eq!(q.len(), 1);

        assert_eq!(q.pop_back(), Some(0));
        assert_eq!(q.len(), 0);

        q.push_back(0).unwrap();
        q.push_back(1).unwrap();
        q.push_front(2).unwrap();
        q.push_front(3).unwrap();
        assert_eq!(q.len(), 4);

        // deque contains: 3 2 0 1
        assert_eq!(q.pop_front(), Some(3));
        assert_eq!(q.len(), 3);
        assert_eq!(q.pop_front(), Some(2));
        assert_eq!(q.len(), 2);
        assert_eq!(q.pop_back(), Some(1));
        assert_eq!(q.len(), 1);
        assert_eq!(q.pop_front(), Some(0));
        assert_eq!(q.len(), 0);

        // deque is now empty
        assert_eq!(q.pop_front(), None);
        assert_eq!(q.pop_back(), None);
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn as_slices() {
        let mut q: Deque<i32, 4> = Deque::new();
        assert_eq!(q.len(), 0);

        q.push_back(0).unwrap();
        q.push_back(1).unwrap();
        q.push_back(2).unwrap();
        q.push_back(3).unwrap();
        assert_eq!(q.as_slices(), (&[0, 1, 2, 3][..], &[][..]));

        q.pop_front().unwrap();
        assert_eq!(q.as_slices(), (&[1, 2, 3][..], &[][..]));

        q.push_back(4).unwrap();
        assert_eq!(q.as_slices(), (&[1, 2, 3][..], &[4][..]));
    }

    #[test]
    fn clear() {
        let mut q: Deque<i32, 4> = Deque::new();
        assert_eq!(q.len(), 0);

        q.push_back(0).unwrap();
        q.push_back(1).unwrap();
        q.push_back(2).unwrap();
        q.push_back(3).unwrap();
        assert_eq!(q.len(), 4);

        q.clear();
        assert_eq!(q.len(), 0);

        q.push_back(0).unwrap();
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn make_contiguous() {
        let mut q: Deque<i32, 4> = Deque::new();
        assert_eq!(q.len(), 0);

        q.push_back(0).unwrap();
        q.push_back(1).unwrap();
        q.push_back(2).unwrap();
        q.push_back(3).unwrap();

        // Deque contains: 0, 1, 2, 3
        assert_eq!(q.pop_front(), Some(0));
        assert_eq!(q.pop_front(), Some(1));

        // Deque contains: ., ., 2, 3
        q.push_back(4).unwrap();

        // Deque contains: 4, ., 2, 3
        assert_eq!(q.as_slices(), ([2, 3].as_slice(), [4].as_slice()));

        assert_eq!(q.make_contiguous(), &[2, 3, 4]);

        // Deque contains: ., 2, 3, 4
        assert_eq!(q.as_slices(), ([2, 3, 4].as_slice(), [].as_slice()));

        assert_eq!(q.pop_front(), Some(2));
        assert_eq!(q.pop_front(), Some(3));
        q.push_back(5).unwrap();
        q.push_back(6).unwrap();

        // Deque contains: 5, 6, ., 4
        assert_eq!(q.as_slices(), ([4].as_slice(), [5, 6].as_slice()));

        assert_eq!(q.make_contiguous(), &[4, 5, 6]);

        // Deque contains: 4, 5, 6, .
        assert_eq!(q.as_slices(), ([4, 5, 6].as_slice(), [].as_slice()));

        assert_eq!(q.pop_front(), Some(4));
        q.push_back(7).unwrap();
        q.push_back(8).unwrap();

        // Deque contains: 8, 5, 6, 7
        assert_eq!(q.as_slices(), ([5, 6, 7].as_slice(), [8].as_slice()));

        assert_eq!(q.make_contiguous(), &[5, 6, 7, 8]);

        // Deque contains: 5, 6, 7, 8
        assert_eq!(q.as_slices(), ([5, 6, 7, 8].as_slice(), [].as_slice()));
    }
}
