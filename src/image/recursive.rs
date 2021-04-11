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

// Automatically implement IntoTuple so we have to generate less code in the macros
impl<I: IntoNonRecursive> IntoTuple<I::NonRecursive> for I {
    fn into_tuple(self) -> <I as IntoNonRecursive>::NonRecursive {
        self.into_non_recursive()
    }
}

//Implement traits for the empty tuple, the macro doesn't handle that
impl IntoRecursive for () {
    type Recursive = NoneMore;
    fn into_recursive(self) -> Self::Recursive { NoneMore }
}

impl IntoNonRecursive for NoneMore {
    type NonRecursive = ();

    fn into_non_recursive(self) -> Self::NonRecursive {
        ()
    }
}

/// Generates the recursive type corresponding to this tuple:
/// ```nocheck
/// gen_recursive_type!(A, B, C)
/// => Recursive<Recursive<Recursive<NoneMore, A>, B>, C>
/// ```
macro_rules! gen_recursive_type {
    () => { NoneMore };
    ($last:ident $(,$not_last:ident)*) => {
        Recursive<gen_recursive_type!($($not_last),*), $last>
    };
}

/// Generates the recursive value corresponding to the given indices:
/// ```nocheck
/// gen_recursive_value(self; 1, 0)
/// => Recursive { inner: Recursive {  inner: NoneMore, value: self.0 }, value: self.1 }
/// ```
macro_rules! gen_recursive_value {
    ($self:ident;) => { NoneMore };
    ($self:ident; $last:tt $(,$not_last:tt)*) => {
        Recursive { inner: gen_recursive_value!($self; $($not_last),*), value: $self.$last }
    };
}

/// Generates the into_tuple value corresponding to the given type names:
/// ```nocheck
/// gen_tuple_value(self; A, B, C)
/// => (self.inner.inner.value, self.inner.value, self.value)
/// ```
macro_rules! gen_tuple_value {
    ($self:ident; $($all:ident),* ) => {
        gen_tuple_value!(@ $self; (); $($all),*  )
    };

    (@ $self:ident; ($($state:expr),*);) => { ($($state .value,)*) };
    (@ $self:ident; ($($state:expr),*); $last:ident $(,$not_last:ident)* ) => {
        gen_tuple_value!(@ $self; ($($state .inner,)* $self); $($not_last),*  )
    };
}

/// Generate the trait implementations given a sequence of type names in both directions and the indices backwards:
/// ```nocheck
/// generate_single(A, B, C; C, B, A; 2, 1, 0)
/// ```
macro_rules! generate_single {
    ( $($name_fwd:ident),* ; $($name_back:ident),* ; $($index_back:tt),*) => {
        impl<$($name_fwd),*> IntoNonRecursive for gen_recursive_type!($($name_back),*) {
            type NonRecursive = ($($name_fwd,)*);
            fn into_non_recursive(self) -> Self::NonRecursive {
                gen_tuple_value!(self; $($name_fwd),*)
            }
        }

        impl<$($name_fwd),*> IntoRecursive for ($($name_fwd,)*) {
            type Recursive = gen_recursive_type!($($name_back),*);
            fn into_recursive(self) -> Self::Recursive {
                gen_recursive_value!(self; $($index_back),*)
            }
        }
    };
}

/// Generate the trait implementations for the given type names and indices and for all smaller sets of types:
/// ```nocheck
/// generate_all_reversed(C, B, A; 2, 1, 0)
/// ```
macro_rules! generate_all_reversed {
    // This macro does most of the work. It re-reverses the type names since we need them in that order for actual code
    // generation. This macro also chops of the last type name and index and recurses to handle the smaller tuples.

    //entry point base case
    ( ; ; ) => { };

    //entry point
    ($($name_back:ident),* ; $($index_back:tt),* ; ) => {
        generate_all_reversed!(@ ; $($name_back),* ; $($name_back),* ; $($index_back),* );
    };

    //re-reverse base case
    (@ $($name_fwd:ident),* ; ; $name_last:ident $(,$name_rest:ident)* ; $index_last:tt $(,$index_rest:tt)* ) => {
        generate_all_reversed!( $($name_rest),* ; $($index_rest),* ; );
        generate_single!( $($name_fwd),* ; $name_last $(,$name_rest)* ; $index_last $(,$index_rest)* );
    };
    //re-reverse intermediate
    (@
        $($name_fwd:ident),* ; $name_last:ident $(,$name_rest:ident)* ;
        $($name_back:ident),* ;
        $($index_back:tt),*
    ) => {
        generate_all_reversed!(@
            $name_last $(,$name_fwd)* ; $($name_rest),* ;
            $($name_back),* ;
            $($index_back),*
        );
    };
}

/// Generate the trait implementations for the given type names and indices and for all smaller sets of types:
/// ```nocheck
/// generate_all(A, B, C; 0, 1, 2)
/// ```
macro_rules! generate_all {
    // The point of this macro is to reverse the input sequences so we can cut off the last values and recurse for the
    // smaller tuple implementations. Cutting off the last element of a sequence directly is not possible.
    // In this macro we use [] to resolve parsing ambiguities caused by , and ; being allowed in :tt tokens

    //entry point
    ($($name_fwd:ident),* ; $($index_fwd:tt),* ; ) => {
        generate_all!(@ ; $($name_fwd),* ; [] ; [$($index_fwd),*] );
    };

    //reverse base case
    (@ $($name_back:ident),* ; ; [$($index_back:tt),*] ; []) => {
        generate_all_reversed!( $($name_back),* ; $($index_back),* ; );
    };
    //reverse intermediate
    (@
        $($name_back:ident),* ; $name_first:ident $(,$name_rest:ident)* ;
        [$($index_back:tt),*] ; [$index_first:tt $(,$index_rest:tt)*]
    ) => {
        generate_all!(@
            $name_first $(,$name_back)* ; $($name_rest),* ;
            [$index_first $(,$index_back)*] ; [$($index_rest),*]
        );
    };
}

generate_all!(
        A, B, C, D, E, F, G, H;
        0, 1, 2, 3, 4, 5, 6, 7;
);