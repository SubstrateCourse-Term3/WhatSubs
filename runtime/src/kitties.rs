use frame_support::{
	decl_module, decl_storage, decl_event, decl_error, ensure, StorageValue, StorageMap,
	Parameter, traits::{Randomness, Currency, ExistenceRequirement, Get},
	weights::SimpleDispatchInfo,
};
use sp_runtime::{traits::{SimpleArithmetic, Bounded, Member}, DispatchError};
use codec::{Encode, Decode};
use sp_io::hashing::blake2_128;
use system::ensure_signed;
use sp_std::result;
use crate::linked_item::{LinkedList, LinkedItem};

pub trait Trait: system::Trait {
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
	type KittyIndex: Parameter + Member + SimpleArithmetic + Bounded + Default + Copy;
	type Currency: Currency<Self::AccountId>;
	type Randomness: Randomness<Self::Hash>;
	// 最大生育年龄
	type MaxBreedingAge: Get<Self::BlockNumber>;
	// 最小生育年龄
	type MinBreedingAge: Get<Self::BlockNumber>;
	// 要随机增加的年龄的最大值
	type MaxLifespanDelta: Get<Self::BlockNumber>;
}

type BalanceOf<T> = <<T as Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance;

#[cfg_attr(feature = "std", derive(Debug, PartialEq, Eq))]
#[derive(Encode, Decode)]
pub struct Kitty<T> where T: Trait {
	pub dna: [u8; 16],
	pub lifespan: T::BlockNumber,
	pub birthday: T::BlockNumber,
}

type KittyLinkedItem<T> = LinkedItem<<T as Trait>::KittyIndex>;
type OwnedKittiesList<T> = LinkedList<OwnedKitties<T>, <T as system::Trait>::AccountId, <T as Trait>::KittyIndex>;

decl_storage! {
	trait Store for Module<T: Trait> as Kitties {
		/// Stores all the kitties, key is the kitty id / index
		pub Kitties get(fn kitties): map T::KittyIndex => Option<Kitty<T>>;
		/// Stores the total number of kitties.
		pub KittiesCount get(fn kitties_count): T::KittyIndex;

		/// the next kitty index
		KittiesNextId: T::KittyIndex;
		pub KittyTombs get(fn kitty_tombs): double_map T::BlockNumber, T::KittyIndex => Option<T::KittyIndex>;

		pub OwnedKitties get(fn owned_kitties): map (T::AccountId, Option<T::KittyIndex>) => Option<KittyLinkedItem<T>>;
		/// Get kitty owner
		pub KittyOwners get(fn kitty_owner): map T::KittyIndex => Option<T::AccountId>;
		/// Get kitty price. None means not for sale.
		pub KittyPrices get(fn kitty_price): map T::KittyIndex => Option<BalanceOf<T>>;
	}
}

decl_event!(
	pub enum Event<T> where
		<T as system::Trait>::AccountId,
		<T as Trait>::KittyIndex,
		Balance = BalanceOf<T>,
	{
		/// A kitty is created. (owner, kitty_id)
		Created(AccountId, KittyIndex),
		/// A kitty is transferred. (from, to, kitty_id)
		Transferred(AccountId, AccountId, KittyIndex),
		/// A kitty is available for sale. (owner, kitty_id, price)
		Ask(AccountId, KittyIndex, Option<Balance>),
		/// A kitty is sold. (from, to, kitty_id, price)
		Sold(AccountId, AccountId, KittyIndex, Balance),
		/// A kitty died.(owner, kitty_id)
		Died(AccountId, KittyIndex),
	}
);

decl_error! {
	pub enum Error for Module<T: Trait> {
		RequiresOwner,//0
		InvalidKittyId,//1
		KittyNotForSale,//2
		PriceTooLow,//3
		KittiesIdOverflow,//4
		RequiresDifferentParents,//5
		Kitty1TooYoung,//6
		Kitty1TooOld,//7
		Kitty2TooYoung,//8
		Kitty2TooOld,//9
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event() = default;

		/// Create a new kitty
		#[weight = SimpleDispatchInfo::FixedNormal(10_000)]
		pub fn create(origin) {
			let sender = ensure_signed(origin)?;
			Self::create_kitty(&sender)?;
		}

		/// Breed kitties
		#[weight = SimpleDispatchInfo::FixedNormal(10_000)]
		pub fn breed(origin, kitty_id_1: T::KittyIndex, kitty_id_2: T::KittyIndex) {
			let sender = ensure_signed(origin)?;
			let new_kitty_id = Self::do_breed(&sender, kitty_id_1, kitty_id_2)?;
			Self::deposit_event(RawEvent::Created(sender, new_kitty_id));
		}

		/// Transfer a kitty to new owner
		#[weight = SimpleDispatchInfo::FixedNormal(10_000)]
		pub fn transfer(origin, to: T::AccountId, kitty_id: T::KittyIndex) {
			let sender = ensure_signed(origin)?;
			ensure!(<OwnedKitties<T>>::exists((&sender, Some(kitty_id))), Error::<T>::RequiresOwner);
			Self::do_transfer(&sender, &to, kitty_id);
			Self::deposit_event(RawEvent::Transferred(sender, to, kitty_id));
		}

		/// Set a price for a kitty for sale
		/// None to delete the kitty
		#[weight = SimpleDispatchInfo::FixedNormal(10_000)]
		pub fn ask(origin, kitty_id: T::KittyIndex, price: Option<BalanceOf<T>>) {
			let sender = ensure_signed(origin)?;
			Self::ask_kitty(&sender, kitty_id, price)?;
		}

		#[weight = SimpleDispatchInfo::FixedNormal(10_000)]
		pub fn buy(origin, kitty_id: T::KittyIndex, price: BalanceOf<T>) {
			let sender = ensure_signed(origin)?;
			Self::buy_kitty(&sender, kitty_id, price)?;
		}

		#[weight = SimpleDispatchInfo::FixedNormal(50_000)]
		fn on_initialize(n: T::BlockNumber) { Self::kitty_initialize(n); }

		#[weight = SimpleDispatchInfo::FixedNormal(0)]
		fn on_finalize(_n: T::BlockNumber) { }

		fn offchain_worker(_n: T::BlockNumber) { }
	}
}

fn combine_dna(dna1: u8, dna2: u8, selector: u8) -> u8 { ((selector & dna1) | (!selector & dna2)) }

impl<T: Trait> Module<T> {
	//noinspection RsBorrowChecker
	fn kitty_initialize(n: T::BlockNumber) {
		let mut i = 0;
		for kitty_id in <KittyTombs<T>>::iter_prefix(n) {
			i += 1;
			let owner = <KittyOwners<T>>::get(kitty_id).unwrap();
			Self::remove_kitty(&owner, kitty_id);
			Self::deposit_event(RawEvent::Died(owner, kitty_id));
		}
		if i > 0 {
			<KittiesCount<T>>::mutate(|v| {
				*v -= i.into();
			});
			<KittyTombs<T>>::remove_prefix(n);
		}
	}

	fn remove_kitty(owner: &T::AccountId, kitty_id: T::KittyIndex) {
		<Kitties<T>>::remove(&kitty_id);
		<KittyOwners<T>>::remove(&kitty_id);
		<KittyPrices<T>>::remove(&kitty_id);
		<OwnedKittiesList<T>>::remove(owner, kitty_id);
	}

	fn create_kitty(sender: &T::AccountId) -> result::Result<(), DispatchError> {
		let kitty_id = Self::next_kitty_id()?;

		// Generate a random 128bit value
		let dna = Self::random_value(sender);

		// Create and store kitty
		Self::insert_kitty(sender, kitty_id, dna);

		Self::deposit_event(RawEvent::Created(sender.clone(), kitty_id));
		Ok(())
	}

	fn gen_kitty_lifespan(sender: &T::AccountId) -> T::BlockNumber {
		let max: T::BlockNumber = T::MaxBreedingAge::get();
		let min: T::BlockNumber = T::MinBreedingAge::get();
		let delta: T::BlockNumber = T::MaxLifespanDelta::get();
		let ran: T::BlockNumber = (u128::from_be_bytes(Self::random_value(sender)) as u32).into();
		(max + min) / 2.into() + (ran % delta)
	}

	//noinspection RsUnresolvedReference
	fn random_value(sender: &T::AccountId) -> [u8; 16] {
		let payload = (
			T::Randomness::random_seed(),
			&sender,
			<system::Module<T>>::extrinsic_index(),
			<system::Module<T>>::block_number(),
		);
		payload.using_encoded(blake2_128)
	}

	fn next_kitty_id() -> result::Result<T::KittyIndex, DispatchError> {
		let mut err = false;
		let kitty_id = KittiesNextId::<T>::mutate(|v| {
			let tmp = *v;
			if tmp == T::KittyIndex::max_value() {
				err = true;
			} else {
				*v += 1.into();
			}
			tmp
		});
		if err {
			return Err(Error::<T>::KittiesIdOverflow.into());
		}
		Ok(kitty_id)
	}

	//noinspection RsBorrowChecker
	fn insert_kitty(owner: &T::AccountId, kitty_id: T::KittyIndex, dna: [u8; 16]) {
		let kitty = Kitty {
			dna,
			lifespan: Self::gen_kitty_lifespan(owner),
			birthday: Self::block_number(),
		};
		// Create and store kitty
		<Kitties<T>>::insert(kitty_id, &kitty);
		KittiesCount::<T>::mutate(|v| {
			*v += 1.into();
		});
		<KittyOwners<T>>::insert(kitty_id, owner.clone());
		<OwnedKittiesList<T>>::append(owner, kitty_id);
		// 保存猫的死亡时间
		<KittyTombs<T>>::insert(kitty.lifespan + kitty.birthday, kitty_id, kitty_id);
	}

	//noinspection RsBorrowChecker
	fn ask_kitty(sender: &T::AccountId, kitty_id: T::KittyIndex, price: Option<BalanceOf<T>>) -> result::Result<(), DispatchError> {
		ensure!(<OwnedKitties<T>>::exists((sender, Some(kitty_id))), Error::<T>::RequiresOwner);

		if let Some(ref price) = price {
			<KittyPrices<T>>::insert(kitty_id, price);
		} else {
			<KittyPrices<T>>::remove(kitty_id);
		}

		Self::deposit_event(RawEvent::Ask(sender.clone(), kitty_id, price));
		Ok(())
	}

	//noinspection RsBorrowChecker
	fn buy_kitty(sender: &T::AccountId, kitty_id: T::KittyIndex, price: BalanceOf<T>) -> result::Result<(), DispatchError> {
		let owner = Self::kitty_owner(kitty_id);
		ensure!(owner.is_some(), Error::<T>::InvalidKittyId);
		let owner = owner.unwrap();

		let kitty_price = Self::kitty_price(kitty_id);
		ensure!(kitty_price.is_some(), Error::<T>::KittyNotForSale);

		let kitty_price = kitty_price.unwrap();
		ensure!(price >= kitty_price, Error::<T>::PriceTooLow);

		T::Currency::transfer(&sender, &owner, kitty_price, ExistenceRequirement::KeepAlive)?;

		<KittyPrices<T>>::remove(kitty_id);

		Self::do_transfer(&owner, &sender, kitty_id);

		Self::deposit_event(RawEvent::Sold(owner, sender.clone(), kitty_id, kitty_price));

		Ok(())
	}

	//noinspection RsBorrowChecker
	fn check_age(kitty1: &Option<Kitty<T>>, kitty2: &Option<Kitty<T>>) -> result::Result<(), DispatchError> {
		let kitty1 = kitty1.as_ref().unwrap();
		let kitty2 = kitty2.as_ref().unwrap();
		let bn = Self::block_number();
		let kitty1_age = bn - kitty1.birthday;
		let kitty2_age = bn - kitty2.birthday;
		let max_breed_age: T::BlockNumber = T::MaxBreedingAge::get();
		let min_breed_age: T::BlockNumber = T::MinBreedingAge::get();

		ensure!(kitty1_age >= min_breed_age, Error::<T>::Kitty1TooYoung);
		ensure!(kitty1_age <= max_breed_age, Error::<T>::Kitty1TooOld);

		ensure!(kitty2_age >= min_breed_age, Error::<T>::Kitty2TooYoung);
		ensure!(kitty2_age <= max_breed_age, Error::<T>::Kitty2TooOld);
		Ok(())
	}

	fn do_breed(sender: &T::AccountId, kitty_id_1: T::KittyIndex, kitty_id_2: T::KittyIndex) -> result::Result<T::KittyIndex, DispatchError> {
		let kitty1 = Self::kitties(kitty_id_1);
		let kitty2 = Self::kitties(kitty_id_2);

		ensure!(kitty1.is_some(), Error::<T>::InvalidKittyId);
		ensure!(kitty2.is_some(), Error::<T>::InvalidKittyId);
		ensure!(kitty_id_1 != kitty_id_2, Error::<T>::RequiresDifferentParents);
		ensure!(Self::kitty_owner(&kitty_id_1).map(|owner| owner == *sender).unwrap_or(false), Error::<T>::RequiresOwner);
		ensure!(Self::kitty_owner(&kitty_id_2).map(|owner| owner == *sender).unwrap_or(false), Error::<T>::RequiresOwner);

		// 检查年龄是否在一定范围内, 确定是否可以繁殖.
		Self::check_age(&kitty1, &kitty2)?;

		let kitty_id = Self::next_kitty_id()?;
		let kitty1_dna = kitty1.unwrap().dna;
		let kitty2_dna = kitty2.unwrap().dna;

		// Generate a random 128bit value
		let selector = Self::random_value(&sender);
		let mut new_dna = [0u8; 16];

		// Combine parents and selector to create new kitty
		for i in 0..kitty1_dna.len() {
			new_dna[i] = combine_dna(kitty1_dna[i], kitty2_dna[i], selector[i]);
		}

		Self::insert_kitty(sender, kitty_id, new_dna);

		Ok(kitty_id)
	}

	//noinspection RsBorrowChecker
	fn do_transfer(from: &T::AccountId, to: &T::AccountId, kitty_id: T::KittyIndex) {
		<OwnedKittiesList<T>>::remove(&from, kitty_id);
		<OwnedKittiesList<T>>::append(&to, kitty_id);
		<KittyOwners<T>>::insert(kitty_id, to);
	}

	#[allow(dead_code)]
	fn set_kitties_id(c: T::KittyIndex) { <KittiesNextId<T>>::put(c); }

	#[inline]
	fn block_number() -> T::BlockNumber { <system::Module<T>>::block_number() }
}

/// Tests for Kitties module
#[cfg(test)]
mod tests {
	use super::*;

	use sp_core::H256;
	#[allow(unused_imports)]
	use frame_support::{impl_outer_origin, assert_ok, parameter_types, weights::Weight};
	use sp_runtime::{
		traits::{BlakeTwo256, IdentityLookup}, testing::Header, Perbill,
	};

	impl_outer_origin! {
		pub enum Origin for Test {}
	}

	// For testing the module, we construct most of a mock runtime. This means
	// first constructing a configuration type (`Test`) which `impl`s each of the
	// configuration traits of modules we want to use.
	#[derive(Clone, Eq, PartialEq, Debug)]
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
	parameter_types! {
		pub const ExistentialDeposit: u64 = 0;
		pub const TransferFee: u64 = 0;
		pub const CreationFee: u64 = 0;
	}
	impl balances::Trait for Test {
		type Balance = u64;
		type OnFreeBalanceZero = ();
		type OnReapAccount = ();
		type OnNewAccount = ();
		type TransferPayment = ();
		type DustRemoval = ();
		type Event = ();
		type ExistentialDeposit = ExistentialDeposit;
		type TransferFee = TransferFee;
		type CreationFee = CreationFee;
	}

	parameter_types! {
		// set breeding age as number of blocks
		pub const MaxBreedingAge: u64 = 5 * 60000 / 2000;
		pub const MinBreedingAge: u64 = 2 * 60000 / 2000;
		pub const MaxLifespanDelta: u64 = 5 * 60_000 / 2000;
	}

	impl Trait for Test {
		type Event = ();
		type KittyIndex = u32;
		type Currency = balances::Module<Test>;
		type Randomness = randomness_collective_flip::Module<Test>;
		type MaxBreedingAge = MaxBreedingAge;
		type MinBreedingAge = MinBreedingAge;
		type MaxLifespanDelta = MaxLifespanDelta;
	}

	type OwnedKittiesListTest = OwnedKittiesList<Test>;
	type OwnedKittiesTest = OwnedKitties<Test>;
	type KittyModule = Module<Test>;

	// This function basically just builds a genesis storage key/value store according to
	// our desired mockup.
	fn new_test_ext() -> sp_io::TestExternalities {
		let mut t = system::GenesisConfig::default().build_storage::<Test>().unwrap();
		balances::GenesisConfig::<Test> {
			balances: vec![
				(1, 10000),
				(2, 10000),
				(3, 10000),
				(4, 10000),
			],
			vesting: vec![],
		}.assimilate_storage(&mut t).unwrap();
		t.into()
	}

	#[test]
	fn create_kitty() {
		new_test_ext().execute_with(|| {
			let _ = KittyModule::create_kitty(&1);
			assert_eq!(1, KittyModule::kitties_count());
			if let Some(kitty) = KittyModule::kitties(0) {
				let v: Vec<u8> = (&kitty.dna[..]).into();
				let b = v.iter().fold(0u128, |sum, &x| { sum + x as u128 });
				assert!(b > 0);
			} else {
				panic!("error")
			}
		});
	}

	#[test]
	fn create_kitty_overflow() {
		new_test_ext().execute_with(|| {
			KittyModule::set_kitties_id(Bounded::max_value());
			let r = KittyModule::create_kitty(&1);
			assert_eq!(r, Err(Error::<Test>::KittiesIdOverflow.into()));
		});
	}

	#[test]
	fn test_transfer() {
		new_test_ext().execute_with(|| {
			let _ = KittyModule::create_kitty(&1);
			assert_eq!(1, KittyModule::kitties_count());
			let _ = KittyModule::transfer(Origin::signed(1), 0, 2);
			assert_eq!(1, KittyModule::kitties_count());
		});
	}

	#[test]
	fn breed_age() {
		new_test_ext().execute_with(|| {
			<system::Module<Test>>::set_extrinsic_index(0);
			let _ = KittyModule::create_kitty(&1);

			<system::Module<Test>>::set_extrinsic_index(1);
			let _ = KittyModule::create_kitty(&1);

			assert_eq!(KittyModule::do_breed(&1, 0, 1), Err(DispatchError::Module {
				index: 0,
				error: 6,
				message: Some("Kitty1TooYoung"),
			}));

			<system::Module<Test>>::set_block_number(<Test as Trait>::MinBreedingAge::get() + MaxBreedingAge::get() * 2 + MaxLifespanDelta::get());

			assert_eq!(KittyModule::do_breed(&1, 0, 1), Err(DispatchError::Module {
				index: 0,
				error: 7,
				message: Some("Kitty1TooOld"),
			}));
		});
	}

	#[test]
	fn test_ask_buy() {
		new_test_ext().execute_with(|| {
			//kitty id 0
			let _ = <Module<Test>>::create(Origin::signed(1));
			//kitty id 1
			let _ = <Module<Test>>::create(Origin::signed(2));
			// ask kitty id 0
			assert_ok!(<Module<Test>>::ask_kitty(&1, 0, Some(1000)));
			assert_eq!(KittyModule::kitty_price(0), Some(1000));
			assert_eq!(OwnedKittiesListTest::collect(&1, None, 100).1, &[0u8.into()]);
			assert_eq!(OwnedKittiesListTest::collect(&2, None, 100).1, &[1u8.into()]);
			assert_eq!(KittyOwners::<Test>::get(0), Some(1));
			assert_eq!(KittyOwners::<Test>::get(1), Some(2));

			assert_ok!(<Module<Test>>::buy_kitty(&2, 0, 1000));
			assert_eq!(OwnedKittiesListTest::collect(&1, None, 100).1.len(), 0);
			assert_eq!(OwnedKittiesListTest::collect(&2, None, 100).1, &[1u8.into(), 0]);
			assert_eq!(KittyModule::kitty_price(0), None);
			assert_eq!(KittyModule::kitty_price(1), None);
			assert_eq!(KittyOwners::<Test>::get(0), Some(2));
			assert_eq!(KittyOwners::<Test>::get(1), Some(2));

			assert_eq!(KittyModule::kitties_count(), 2);
			let kitty1 = Kitties::<Test>::get(0).unwrap();
			let kitty2 = Kitties::<Test>::get(1).unwrap();
			Module::<Test>::kitty_initialize(kitty1.lifespan + kitty1.birthday);
			Module::<Test>::kitty_initialize(kitty2.lifespan + kitty2.birthday);
			assert_eq!(KittyOwners::<Test>::get(0), None);
			assert_eq!(KittyOwners::<Test>::get(1), None);
			assert_eq!(Kitties::<Test>::get(0), None);
			assert_eq!(Kitties::<Test>::get(1), None);
			assert_eq!(KittyModule::kitties_count(), 0);
			assert_eq!(OwnedKittiesListTest::collect(&1, None, 100).1.len(), 0);
			assert_eq!(OwnedKittiesListTest::collect(&2, None, 100).1.len(), 0);
		});
	}

	#[test]
	fn breed_kitty() {
		new_test_ext().execute_with(|| {
			<system::Module<Test>>::set_extrinsic_index(0);
			let _ = KittyModule::create_kitty(&1);

			<system::Module<Test>>::set_extrinsic_index(1);
			let _ = KittyModule::create_kitty(&1);

			assert_eq!(2, KittyModule::kitties_count());

			<system::Module<Test>>::set_block_number(<Test as Trait>::MinBreedingAge::get() + 1);

			assert_eq!(KittyModule::do_breed(&1, 0, 1), Ok(2));
			assert_eq!(3, KittyModule::kitties_count());
			let dna1 = KittyModule::kitties(0).unwrap().dna;
			let dna2 = KittyModule::kitties(1).unwrap().dna;
			let dna3 = KittyModule::kitties(2).unwrap().dna;
			assert_ne!(dna1, dna2);
			assert_ne!(dna1, dna3);
			assert_ne!(dna2, dna3);
		});
	}

	#[test]
	fn owned_kitties_can_append_values() {
		new_test_ext().execute_with(|| {
			OwnedKittiesList::<Test>::append(&0, 1);

			assert_eq!(OwnedKittiesTest::get(&(0, None)), Some(KittyLinkedItem::<Test> {
				prev: Some(1),
				next: Some(1),
			}));

			assert_eq!(OwnedKittiesTest::get(&(0, Some(1))), Some(KittyLinkedItem::<Test> {
				prev: None,
				next: None,
			}));

			OwnedKittiesList::<Test>::append(&0, 2);

			assert_eq!(OwnedKittiesTest::get(&(0, None)), Some(KittyLinkedItem::<Test> {
				prev: Some(2),
				next: Some(1),
			}));

			assert_eq!(OwnedKittiesTest::get(&(0, Some(1))), Some(KittyLinkedItem::<Test> {
				prev: None,
				next: Some(2),
			}));

			assert_eq!(OwnedKittiesTest::get(&(0, Some(2))), Some(KittyLinkedItem::<Test> {
				prev: Some(1),
				next: None,
			}));

			OwnedKittiesList::<Test>::append(&0, 3);

			assert_eq!(OwnedKittiesTest::get(&(0, None)), Some(KittyLinkedItem::<Test> {
				prev: Some(3),
				next: Some(1),
			}));

			assert_eq!(OwnedKittiesTest::get(&(0, Some(1))), Some(KittyLinkedItem::<Test> {
				prev: None,
				next: Some(2),
			}));

			assert_eq!(OwnedKittiesTest::get(&(0, Some(2))), Some(KittyLinkedItem::<Test> {
				prev: Some(1),
				next: Some(3),
			}));

			assert_eq!(OwnedKittiesTest::get(&(0, Some(3))), Some(KittyLinkedItem::<Test> {
				prev: Some(2),
				next: None,
			}));

			assert_eq!(OwnedKittiesListTest::collect(&0, None, 100), (Some(3), vec![1u8.into(), 2, 3]));
		});
	}

	#[test]
	fn owned_kitties_can_remove_values() {
		new_test_ext().execute_with(|| {
			OwnedKittiesList::<Test>::append(&0, 1);
			OwnedKittiesList::<Test>::append(&0, 2);
			OwnedKittiesList::<Test>::append(&0, 3);

			OwnedKittiesList::<Test>::remove(&0, 2);

			assert_eq!(OwnedKittiesTest::get(&(0, None)), Some(KittyLinkedItem::<Test> {
				prev: Some(3),
				next: Some(1),
			}));

			assert_eq!(OwnedKittiesTest::get(&(0, Some(1))), Some(KittyLinkedItem::<Test> {
				prev: None,
				next: Some(3),
			}));

			assert_eq!(OwnedKittiesTest::get(&(0, Some(2))), None);

			assert_eq!(OwnedKittiesTest::get(&(0, Some(3))), Some(KittyLinkedItem::<Test> {
				prev: Some(1),
				next: None,
			}));

			OwnedKittiesList::<Test>::remove(&0, 1);

			assert_eq!(OwnedKittiesTest::get(&(0, None)), Some(KittyLinkedItem::<Test> {
				prev: Some(3),
				next: Some(3),
			}));

			assert_eq!(OwnedKittiesTest::get(&(0, Some(1))), None);

			assert_eq!(OwnedKittiesTest::get(&(0, Some(2))), None);

			assert_eq!(OwnedKittiesTest::get(&(0, Some(3))), Some(KittyLinkedItem::<Test> {
				prev: None,
				next: None,
			}));

			OwnedKittiesList::<Test>::remove(&0, 3);

			assert_eq!(OwnedKittiesTest::get(&(0, None)), Some(KittyLinkedItem::<Test> {
				prev: None,
				next: None,
			}));

			assert_eq!(OwnedKittiesTest::get(&(0, Some(1))), None);

			assert_eq!(OwnedKittiesTest::get(&(0, Some(2))), None);

			assert_eq!(OwnedKittiesTest::get(&(0, Some(2))), None);
		});
	}
}
