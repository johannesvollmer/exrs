//! A generic wrapper which can be used to represent recursive types.
//! Supports conversion from and to tuples of the same size.

/// No more recursion. Can be used within any `Recursive<NoneMore, YourValue>` type.
#[derive(Copy, Clone, Debug, Default)]
pub struct NoneMore;

/// A recursive type-level linked list of `Value` entries.
/// Mainly used to represent an arbitrary number of channels.
/// The recursive architecture removes the need to implement traits for many different tuples.
#[derive(Copy, Clone, Debug, Default)]
pub struct Recursive<Inner, Value> {

    /// The remaining values of this linked list,
    /// probably either `NoneMore` or another instance of the same `Recursive<Inner - 1, Value>`.
    pub inner: Inner,

    /// The next item in this linked list.
    pub value: Value,
}

impl<Inner, Value> Recursive<Inner, Value> {
    /// Create a new recursive type. Equivalent to the manual constructor, but less verbose.
    pub fn new(inner: Inner, value: Value) -> Self { Self { inner, value } }
}

/// Convert this recursive type into a tuple.
/// This is nice as it will require less typing for the same type.
/// A type might or might not be convertible to the specified `Tuple` type.
pub trait IntoTuple<Tuple> {

    /// Convert this recursive type to a nice tuple.
    fn into_tuple(self) -> Tuple;
}

/// Convert this recursive type into a tuple.
/// This is nice as it will require less typing for the same type.
/// A type will be converted to the specified `Self::NonRecursive` type.
pub trait IntoNonRecursive {

    /// The resulting tuple type.
    type NonRecursive;

    /// Convert this recursive type to a nice tuple.
    fn into_non_recursive(self) -> Self::NonRecursive;
}

/// Create a recursive type from this tuple.
pub trait IntoRecursive {

    /// The recursive type resulting from this tuple.
    type Recursive;

    /// Create a recursive type from this tuple.
    fn into_recursive(self) -> Self::Recursive;
}

impl IntoRecursive for NoneMore {
    type Recursive = Self;
    fn into_recursive(self) -> Self::Recursive { self }
}

impl<Inner: IntoRecursive, Value> IntoRecursive for Recursive<Inner, Value> {
    type Recursive = Recursive<Inner::Recursive, Value>;
    fn into_recursive(self) -> Self::Recursive { Recursive::new(self.inner.into_recursive(), self.value) }
}

// TODO use a macro to generate these impls!
impl IntoTuple<()> for NoneMore { fn into_tuple(self) -> () { () } }
impl<A> IntoTuple<(A,)> for Recursive<NoneMore, A> { fn into_tuple(self) -> (A,) { (self.value,) } }
impl<A,B> IntoTuple<(A,B)> for Recursive<Recursive<NoneMore, A>, B> { fn into_tuple(self) -> (A, B) { (self.inner.value, self.value) } }
impl<A,B,C> IntoTuple<(A,B,C)> for Recursive<Recursive<Recursive<NoneMore, A>, B>, C> { fn into_tuple(self) -> (A, B, C) { (self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D> IntoTuple<(A,B,C,D)> for Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D> { fn into_tuple(self) -> (A, B, C, D) { (self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D,E> IntoTuple<(A,B,C,D,E)> for Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E> { fn into_tuple(self) -> (A, B, C, D, E) { (self.inner.inner.inner.inner.value, self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D,E,F> IntoTuple<(A,B,C,D,E,F)> for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F> { fn into_tuple(self) -> (A, B, C, D, E, F) { (self.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.value, self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D,E,F,G> IntoTuple<(A,B,C,D,E,F,G)> for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>, G> { fn into_tuple(self) -> (A, B, C, D, E, F, G) { (self.inner.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.value, self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D,E,F,G,H> IntoTuple<(A,B,C,D,E,F,G,H)> for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>, G>, H> { fn into_tuple(self) -> (A, B, C, D, E, F, G, H) { (self.inner.inner.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.value, self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }

// impl<AsTuple, Tuple> IntoNonRecursive for AsTuple where AsTuple: IntoTuple<Tuple> {
//     type NonRecursive = Tuple;
//     fn into_friendlier(self) -> Self::NonRecursive { self.into_tuple() }
// }
impl IntoNonRecursive for NoneMore { type NonRecursive = (); fn into_non_recursive(self) -> Self::NonRecursive { () } }
impl<A> IntoNonRecursive for Recursive<NoneMore, A> { type NonRecursive = (A,); fn into_non_recursive(self) -> Self::NonRecursive { (self.value,) } }
impl<A,B> IntoNonRecursive for Recursive<Recursive<NoneMore, A>, B> { type NonRecursive = (A, B); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C> IntoNonRecursive for Recursive<Recursive<Recursive<NoneMore, A>, B>, C> { type NonRecursive = (A, B, C); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D> { type NonRecursive = (A, B, C, D); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D,E> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E> { type NonRecursive = (A, B, C, D, E); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D,E,F> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F> { type NonRecursive = (A, B, C, D, E, F); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D,E,F,G> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>, G> { type NonRecursive = (A, B, C, D, E, F, G); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D,E,F,G,H> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>, G>, H> { type NonRecursive = (A, B, C, D, E, F, G, H); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }

impl IntoRecursive for () { type Recursive = NoneMore; fn into_recursive(self) -> Self::Recursive { NoneMore } }
impl<A> IntoRecursive for (A,) { type Recursive = Recursive<NoneMore, A>; fn into_recursive(self) -> Self::Recursive { Recursive::new(NoneMore, self.0) } }
impl<A,B> IntoRecursive for (A,B) { type Recursive = Recursive<Recursive<NoneMore, A>, B>; fn into_recursive(self) -> Self::Recursive { Recursive::new((self.0,).into_recursive(), self.1) } }
impl<A,B,C> IntoRecursive for (A,B,C) { type Recursive = Recursive<Recursive<Recursive<NoneMore, A>, B>, C>; fn into_recursive(self) -> Self::Recursive { Recursive::new((self.0,self.1).into_recursive(), self.2) } }
impl<A,B,C,D> IntoRecursive for (A,B,C,D) { type Recursive = Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>; fn into_recursive(self) -> Self::Recursive { Recursive::new((self.0,self.1,self.2).into_recursive(), self.3) } }
impl<A,B,C,D,E> IntoRecursive for (A,B,C,D,E) { type Recursive = Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>; fn into_recursive(self) -> Self::Recursive { Recursive::new((self.0,self.1,self.2,self.3).into_recursive(), self.4) } }
impl<A,B,C,D,E,F> IntoRecursive for (A,B,C,D,E,F) { type Recursive = Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>; fn into_recursive(self) -> Self::Recursive { Recursive::new((self.0,self.1,self.2,self.3,self.4).into_recursive(), self.5) } }
impl<A,B,C,D,E,F,G> IntoRecursive for (A,B,C,D,E,F,G) { type Recursive = Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>, G>; fn into_recursive(self) -> Self::Recursive { Recursive::new((self.0,self.1,self.2,self.3,self.4,self.5).into_recursive(), self.6) } }
impl<A,B,C,D,E,F,G,H> IntoRecursive for (A,B,C,D,E,F,G,H) { type Recursive = Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>, G>, H>; fn into_recursive(self) -> Self::Recursive { Recursive::new((self.0,self.1,self.2,self.3,self.4,self.5,self.6).into_recursive(), self.7) } }
// TODO more

/*macro_rules! impl_into_tuple_for_recursive_type {
    ( $($types: ident),* => => $nested_type:ty => $($accessors:expr),* => /*empty $acessor_prefix:expr*/ ) => {
        impl<  $($types),*  > IntoTuple<(  $($types),*  )> for $nested_type {
            fn into_tuple(self) -> (  $($types),*  ) { (  $($accessors),*  ) }
        }
    };

    ( $($types: ident),* => $last_type:ident, $($remaining_types:ident),* => $nested_type:ty => $($accessors:expr),* => $acessor_prefix:expr ) => {
        impl_into_tuple_for_recursive_type!{
            $($types),* =>
            $($remaining_types),* =>
            Recursive< $nested_type, $last_type > =>
            $acessor_prefix .value, $($accessors),* =>
            $acessor_prefix .inner
        }
    };

    ( $($types:ident),* ) => {
        impl_into_tuple_for_recursive_type!{
            $($types),* => $($types),* => NoneMore => => self.value
        }
    };
}*/

/*macro_rules! gen_impl {

    ( IntoTuple:nested_type: $inner_recursive:ty |   ) => {
        $inner_recursive<>
    };
    ( IntoTuple:nested_type: $inner_recursive:ty | $first_chan:ident $(,$remaining_chans:ident)*  ) => {
        gen_impl!(IntoTuple:nested_type: Recursive<$inner_recursive, $first_chan> | $($remaining_chans),* )
    };

    ( IntoTuple:accessors: $self:ident | $($types:ident),* ) => {
        ($self.inner.inner.inner.value, $self.inner.inner.value, $self.inner.value, $self.value)
    };

    ( IntoTuple: $($types: ident),* ) => {
        impl<  $($types),*  > IntoTuple<(  $($types),*  )> for ( gen_impl!( IntoTuple:nested_type: $($types),* ) ) {
            fn into_tuple(self) -> (  $($types),*  ) {
                gen_impl!( IntoTuple:accessors: self | $($types),* )
            }
        }
    };

}

gen_impl! {
    IntoTuple:
    A,B,C,D
}*/

//impl_into_tuple_for_recursive_type! { A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P,Q,R,S,T }

/*macro_rules! impl_into_tuple_for_recursive_type_all {

    // internal initial call
    ( $types:expr ) => {
        impl_into_tuple_for_recursive_type!{
            $types => $types => NoneMore => => self.value
        }
    };

    // initial call
    ( $( $types:expr );* ) => {
        impl_into_tuple_for_recursive_type_all!{
            $($types),* => $($types),* => NoneMore => => self.value
        }
    };
}

// impl for sizes 2,3,4,5,6,7,8,12,16,20.
impl_into_tuple_for_recursive_type_all! {
    A,B; A,B,C; A,B,C,D; A,B,C,D,E; A,B,C,D,E,F; A,B,C,D,E,F,G; A,B,C,D,E,F,G,H;
    A,B,C,D,E,F,G,H,I,J,K,L; A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P; A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P,Q,R,S,T;
}*/


