#![allow(unused_imports)]
#![allow(dead_code)]
use sp_std::{vec::Vec, result::Result};
use frame_support::{Parameter, StorageMap, StorageValue, ensure};
use sp_runtime::traits::{SimpleArithmetic, Bounded};
use sp_std::marker::PhantomData;

/*
### binary heap
* runtime/src/binary_heap.rs
* 实现了最小堆, 参考了Rust标准库中的堆实现.
* 如果有10000只猫, 树高14层.
* 通过测试
*/

pub struct MinHeap<Index, Value, Count, Map> (PhantomData<(Index, Value, Count, Map)>);

impl<Index, Value, Count, Map> MinHeap<Index, Value, Count, Map>
	where Index: Parameter + SimpleArithmetic + Bounded + Default + Copy + PartialEq,
		  Value: Parameter + PartialOrd,
		  Count: StorageValue<Index, Query=Index>,
		  Map: StorageMap<Index, Value, Query=Option<Value>> {
	pub fn len() -> Index { Count::get() }

	pub fn is_empty() -> bool { Self::len() == Index::zero() }

	pub fn peek() -> Option<Value> { Map::get(Index::zero()) }

	pub fn push(item: Value) -> Result<(), &'static str> {
		let old_len = Count::get();
		if old_len == Index::max_value() {
			return Err("heap items len overflow");
		}
		Count::put(old_len + Index::one());
		Self::sift_up(Index::zero(), old_len, item);
		Ok(())
	}

	fn sift_up(start: Index, pos: Index, item: Value) {
		let mut hole: Hole<Value, Index, Map> = Hole::new(pos, &item);
		while hole.pos() > start {
			let parent = (hole.pos() - Index::one()) / 2u8.into();
			let parent_item = hole.get(parent).unwrap();
			if *hole.element() >= parent_item {
				break;
			}
			hole.move_to(parent, &parent_item);
		}
	}

	pub fn pop() -> Option<Value> {
		let mut len = Self::len();
		if len == Index::zero() {
			return None;
		}
		len = len - Index::one();
		Count::put(len);
		if len == Index::zero() {
			Map::take(Index::zero())
		} else {
			let top_one = Map::get(Index::zero());
			let last = Map::take(len).unwrap();
			Self::sift_down_range(last, Index::zero(), len);
			top_one
		}
	}

	fn sift_down_range(last_item: Value, pos: Index, end: Index) {
		let mut hole: Hole<Value, Index, Map> = Hole::new(pos, &last_item);
		let mut child = pos * 2u8.into() + Index::one();
		while child < end {
			let mut child_item = hole.get(child).unwrap();
			let right = child + Index::one();
			if right < end {
				let right_item = hole.get(right).unwrap();
				if !(child_item < right_item) {
					child = right;
					child_item = right_item;
				}
			}
			if *hole.element() <= child_item {
				break;
			}
			hole.move_to(child, &child_item);
			child = hole.pos() * 2u8.into() + Index::one();
		}
	}
}

struct Hole<'a, Value: 'a, Index, Map>
	where Index: Parameter + SimpleArithmetic + Bounded + Default + Copy + PartialEq,
		  Value: Parameter + PartialOrd,
		  Map: StorageMap<Index, Value, Query=Option<Value>> {
	pos: Index,
	elt: &'a Value,
	phantom: PhantomData<Map>,
}

impl<'a, Value: 'a, Index, Map> Hole<'a, Value, Index, Map>
	where Index: Parameter + SimpleArithmetic + Bounded + Default + Copy + PartialEq,
		  Value: Parameter + PartialOrd,
		  Map: StorageMap<Index, Value, Query=Option<Value>> {
	#[inline]
	fn new(pos: Index, elt: &'a Value) -> Self { Hole { pos, elt, phantom: PhantomData } }

	#[inline]
	fn pos(&self) -> Index { self.pos }

	#[inline]
	fn element(&self) -> &Value { self.elt }

	#[inline]
	fn get(&self, index: Index) -> Option<Value> { Map::get(index) }

	#[inline]
	fn move_to(&mut self, index: Index, value: &Value) {
		Map::insert(self.pos, value);
		self.pos = index;
	}
}

impl<Value, Index, Map> Drop for Hole<'_, Value, Index, Map>
	where Index: Parameter + SimpleArithmetic + Bounded + Default + Copy + PartialEq,
		  Value: Parameter + PartialOrd,
		  Map: StorageMap<Index, Value, Query=Option<Value>> {
	#[inline]
	fn drop(&mut self) {
		Map::insert(self.pos, self.elt);
	}
}

//noinspection ALL
/// tests for this module
#[cfg(test)]
mod tests {
	use super::MinHeap;

	use sp_core::H256;
	use frame_support::{impl_outer_origin, assert_ok, parameter_types, weights::Weight};
	use sp_runtime::{
		traits::{BlakeTwo256, IdentityLookup}, testing::Header, Perbill,
	};
	use frame_support::{decl_module, decl_storage, decl_event, dispatch};

	impl_outer_origin! {
		pub enum Origin for Test {}
	}

	// For testing the module, we construct most of a mock runtime. This means
	// first constructing a configuration type (`Test`) which `impl`s each of the
	// configuration traits of modules we want to use.
	#[derive(Clone, Eq, PartialEq)]
	pub struct Test;
	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub const MaximumBlockWeight: Weight = 1024;
		pub const MaximumBlockLength: u32 = 2 * 1024;
		pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
	}

	impl system::Trait for Test {
		type Origin = Origin;
		type Call = ();
		type Index = u64;
		type BlockNumber = u64;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = ();
		type BlockHashCount = BlockHashCount;
		type MaximumBlockWeight = MaximumBlockWeight;
		type MaximumBlockLength = MaximumBlockLength;
		type AvailableBlockRatio = AvailableBlockRatio;
		type Version = ();
		type ModuleToIndex = ();
	}

	impl Trait for Test {}

	pub trait Trait: system::Trait {}

	decl_storage! {
		trait Store for Module<T: Trait> as TemplateModule {
			Count get(fn count): u32;
			Data get(fn data): map u32 => Option<u32>;
		}
	}

	decl_module! {
		pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		}
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
	}

	type Heap = MinHeap<u32, u32, Count, Data>;

	fn test() {
		assert!(Heap::is_empty());
		let _ = Heap::push(1);
		let _ = Heap::push(5);
		let _ = Heap::push(6);
		let _ = Heap::push(9);
		let _ = Heap::push(11);
		let _ = Heap::push(8);
		let _ = Heap::push(15);
		let _ = Heap::push(21);
		let _ = Heap::push(17);
		let _ = Heap::push(3);
		let _ = Heap::push(3);
		let _ = Heap::push(3);
		let _ = Heap::pop();
		let _ = Heap::pop();
		let _ = Heap::pop();
		let _ = Heap::pop();
		let _ = Heap::push(1);
		let _ = Heap::push(3);
		let _ = Heap::push(3);
		let _ = Heap::push(3);

		assert_eq!(Heap::peek(), Some(1));
		assert_eq!(Heap::len(), 12);
		for i in 0..Heap::len() + 2 {
			print!("{:?}  ", Data::get(i));
		}
		println!();
		let mut count = 0;
		let mut item = 0;
		while let Some(x) = Heap::pop() {
			count += 1;
			assert!(item <= x, "error");
			item = x;
			print!("{}  ", x);
		}
		assert_eq!(count, 12);
	}

	#[test]
	fn test_2020_01_13_14_05_13() {
		new_test_ext().execute_with(|| {
			for _ in 0..3 {
				test();
				println!();
			}
		});
	}

	#[test]
	fn test_2020_01_14_23_29_11() {
		use rand::SeedableRng;
		use rand::Rng;
		new_test_ext().execute_with(|| {
			let c = 10;
			for _ in 0..c {
				let rnd: [u32; 32] = rand::random();
				for x in rnd.iter() {
					let _ = Heap::push(*x);
				}
			}
			assert_eq!(Heap::len(), 32 * c);

			let mut count = 0;
			let mut item = 0;
			while let Some(x) = Heap::pop() {
				count += 1;
				assert!(item <= x, "error");
				item = x;
				println!("{}", item);
			}
			assert_eq!(count, 32 * c);
		});
	}

	#[test]
	fn test_2020_01_15_00_00_58() {
		new_test_ext().execute_with(|| {
			for x in 0..320 {
				let _ = Heap::push(x);
			}
			let mut item = 0;
			while let Some(x) = Heap::pop() {
				assert!(item <= x, "error");
				item = x;
				println!("{}", item);
			}
		})
	}
}

