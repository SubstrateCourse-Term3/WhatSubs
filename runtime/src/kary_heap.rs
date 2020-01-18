#![allow(unused_imports)]
#![allow(dead_code)]

use frame_support::{Parameter, StorageMap, StorageValue, ensure};
use sp_runtime::traits::{SimpleArithmetic, Bounded};
use sp_std::marker::PhantomData;
use sp_std::{result::Result};
use sp_std::convert::TryInto;
use sp_std::cmp::Ordering;

/*
### k-ary heap
* runtime/src/kary_heap.rs
* 实现了多叉最小堆, 参考了Rust标准库中的堆实现.
* 如果有10000只猫, k=8时, 树高5层.
* 每个节点的所有子节点, 保存在一起于一个数组中.
* 由于堆保存于磁盘. 减少树高能减少磁盘的IO.
* 通过测试
*/

// 改为注入的方式
pub const KARY: u8 = 8;

pub struct MinHeapKAry<Index, Value, Count, Map> (PhantomData<(Index, Value, Count, Map)>);

impl<Index, Value, Count, Map> MinHeapKAry<Index, Value, Count, Map>
	where Index: Parameter + SimpleArithmetic + Bounded + Default + Copy + PartialEq,
		  Value: Parameter + PartialOrd + Ord + Default + Copy,
		  <Index as sp_std::convert::TryInto<usize>>::Error: sp_std::fmt::Debug,
		  Count: StorageValue<Index, Query=Index>,
		  Map: StorageMap<Index, [Option<Value>; KARY as usize], Query=Option<[Option<Value>; KARY as usize]>> {
	pub fn len() -> Index { Count::get() }

	pub fn is_empty() -> bool { Self::len() == Index::zero() }

	pub fn peek() -> Option<Value> {
		Map::get(Index::zero()).map(|v| {
			v[0].unwrap()
		})
	}

	pub fn push(item: Value) -> Result<(), &'static str> {
		let old_len = Count::get();
		if old_len == Index::max_value() {
			return Err("heap items len overflow");
		}
		Count::put(old_len + Index::one());
		Self::sift_up(old_len, item);
		Ok(())
	}

	fn sift_up(pos: Index, item: Value) {
		let mut hole: HoleKAry<Value, Index, Map> = HoleKAry::new(pos, &item);
		while hole.pos() > Index::zero() {
			let parent = (hole.pos() - Index::one()) / KARY.into();
			let (arr_of_parent, parent_in_arr) = hole.get(parent);
			let parent_value = arr_of_parent[parent_in_arr].unwrap();
			if *hole.element() >= parent_value {
				break;
			}
			hole.move_to(parent, parent_value);
		}
		hole.fill();
	}

	pub fn pop() -> Option<Value> {
		let mut len = Self::len();
		if len == Index::zero() {
			return None;
		}
		len = len - Index::one();
		Count::put(len);
		if len == Index::zero() {
			Map::take(Index::zero()).map(|v| {
				v[0].unwrap()
			})
		} else {
			let top_one = Self::peek();
			let last = Map::mutate(index2slot(len), |arr| {
				let mut v = arr.unwrap();
				let i = index2pos_in_slot(len);
				let last = v[i].unwrap();
				if i == 0 {
					*arr = None
				} else {
					v[i] = None;
					*arr = Some(v);
				}
				last
			});
			Self::sift_down_range(last, Index::zero(), len);
			top_one
		}
	}

	fn sift_down_range(last_item: Value, pos: Index, end: Index) {
		let mut hole: HoleKAry<Value, Index, Map> = HoleKAry::new(pos, &last_item);
		let mut child = pos * KARY.into() + Index::one();
		while child < end {
			let (children, _) = hole.get(child);
			let (child_pos_in_arr, child_item) = children.iter().enumerate().min_by(|&(_, &x), &(_, &y)| {
				match (x, y) {
					(None, None) => Ordering::Equal,
					(None, _) => Ordering::Greater,
					(_, None) => Ordering::Less,
					(Some(x), Some(y)) => x.cmp(&y),
				}
			}).unwrap();
			if *hole.element() <= child_item.unwrap() {
				break;
			}
			hole.move_to(child + (child_pos_in_arr as u32).into(), child_item.unwrap());
			child = hole.pos() * KARY.into() + Index::one();
		}
		hole.fill();
	}
}

#[inline]
fn index2slot<Index>(i: Index) -> Index where Index: Parameter + SimpleArithmetic + Bounded + Default + Copy + PartialEq {
	if i == Index::zero() {
		i
	} else {
		(i - Index::one()) / KARY.into() + Index::one()
	}
}

#[inline]
fn index2pos_in_slot<Index>(i: Index) -> usize
	where Index: Parameter + SimpleArithmetic + Bounded + Default + Copy + PartialEq,
		  <Index as sp_std::convert::TryInto<usize>>::Error: sp_std::fmt::Debug {
	if i == Index::zero() {
		0
	} else {
		((i - Index::one()) % KARY.into()).try_into().unwrap()
	}
}

struct HoleKAry<'a, Value: 'a, Index, Map> {
	pos: Index,
	elt: &'a Value,
	phantom: PhantomData<Map>,
}

impl<'a, Value: 'a, Index, Map> HoleKAry<'a, Value, Index, Map>
	where Index: Parameter + SimpleArithmetic + Bounded + Default + Copy + PartialEq,
		  Value: Parameter + PartialOrd + Copy,
		  Map: StorageMap<Index, [Option<Value>; KARY as usize], Query=Option<[Option<Value>; KARY as usize]>>,
		  <Index as sp_std::convert::TryInto<usize>>::Error: sp_std::fmt::Debug {
	#[inline]
	fn new(pos: Index, elt: &'a Value) -> Self { HoleKAry { pos, elt, phantom: PhantomData } }

	#[inline]
	fn pos(&self) -> Index { self.pos }

	#[inline]
	fn element(&self) -> &Value { self.elt }

	//noinspection RsTypeCheck
	#[inline]
	fn get(&self, index: Index) -> ([Option<Value>; KARY as usize], usize) {
		(Map::get(index2slot(index)).unwrap(), index2pos_in_slot(index))
	}

	//noinspection RsTypeCheck
	#[inline]
	fn move_to(&mut self, index: Index, value: Value) {
		Map::mutate(index2slot(self.pos), |this_arr| {
			match this_arr {
				None => {
					let mut a = [None; KARY as usize];
					a[0] = Some(value.clone());
					*this_arr = Some(a);
				}
				Some(this_arr) => {
					let pos: usize = index2pos_in_slot(self.pos);
					this_arr[pos] = Some(value.clone());
				}
			}
		});
		self.pos = index;
	}

	//noinspection RsTypeCheck
	#[inline]
	fn fill(&self) {
		Map::mutate(index2slot(self.pos), |arr| {
			match arr {
				None => {
					let mut a = [None; KARY as usize];
					a[0] = Some(self.elt.clone());
					*arr = Some(a);
				}
				Some(this_arr) => {
					let pos: usize = index2pos_in_slot(self.pos);
					this_arr[pos] = Some(self.elt.clone());
				}
			}
		});
	}
}

//noinspection ALL
/// tests for this module
#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Encode, Decode};

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
			Data get(fn data): map u32 => Option<[Option<u32>;KARY as usize]>;
		}
	}

	decl_module! {
		pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		}
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
	}

	type Heap = MinHeapKAry<u32, u32, Count, Data>;

	#[test]
	fn test_2020_01_14_23_31_28() {
		let children_count = 8;
		let span_size = children_count;
		let mut span = 0;
		for i in 0..100 {
			for j in 1..=children_count {
				let mut bias = 0;
				if i + j > 0 {
					bias = 1;
				}
				let k = i * children_count + j;
				let new_span = (k - bias) / span_size + bias;
				if span != new_span {
					println!("-------------");
				}
				span = new_span;
				assert_eq!((k - 1) / children_count, i);
				println!("{} {} {} {}", (k - 1) / children_count, k, span, (k - bias) % children_count)
			}
		}
	}

	fn display() {
		let mut i: u32 = 0;
		while let Some(a) = Data::get(i) {
			println!("{:?}", a);
			i += 1;
		}
		println!("---------------")
	}

	fn test() {
		assert_eq!(Heap::len(), 0u32.into());
		assert_eq!(Heap::peek(), None);
		let _ = Heap::push(5);
		let _ = Heap::push(1);
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
		assert_eq!(Heap::len(), 12u32.into());
		assert_eq!(Heap::peek(), Some(1));
		assert_eq!(Heap::pop(), Some(1));
		assert_eq!(Heap::pop(), Some(3));
		assert_eq!(Heap::pop(), Some(3));
		assert_eq!(Heap::pop(), Some(3));
		assert_eq!(Heap::pop(), Some(5));
		assert_eq!(Heap::pop(), Some(6));
		assert_eq!(Heap::pop(), Some(8));
		assert_eq!(Heap::pop(), Some(9));
		assert_eq!(Heap::pop(), Some(11));
		assert_eq!(Heap::pop(), Some(15));
		assert_eq!(Heap::pop(), Some(17));
		assert_eq!(Heap::pop(), Some(21));
		assert_eq!(Heap::pop(), None);
	}

	#[test]
	fn test_2020_01_13_14_05_13() {
		new_test_ext().execute_with(|| {
			for _ in 0..3 {
				test();
			}
		});
	}

	#[test]
	fn test_2020_01_15_00_00_58() {
		new_test_ext().execute_with(|| {
			for x in 0..32 {
				let _ = Heap::push(x);
			}
			display();
			let mut item = 0;
			while let Some(x) = Heap::pop() {
				assert!(item <= x, "error");
				item = x;
				display();
			}
		})
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
	fn test_2020_01_14_23_29_12() {
		use rand::SeedableRng;
		use rand::Rng;
		new_test_ext().execute_with(|| {
			for _ in 0..3 {
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
				}
				assert_eq!(count, 32 * c);
			}
		});
	}
}

