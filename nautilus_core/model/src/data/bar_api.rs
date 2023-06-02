// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2023 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

use std::collections::hash_map::DefaultHasher;
use std::ffi::c_char;
use std::hash::{Hash, Hasher};

use nautilus_core::string::str_to_cstr;

use crate::enums::{AggregationSource, BarAggregation, PriceType};
use crate::identifiers::instrument_id::InstrumentId;

use super::bar::{Bar, BarSpecification, BarType};

/// Returns a [`BarSpecification`] as a C string pointer.
#[no_mangle]
pub extern "C" fn bar_specification_to_cstr(bar_spec: &BarSpecification) -> *const c_char {
    str_to_cstr(&bar_spec.to_string())
}

#[no_mangle]
pub extern "C" fn bar_specification_hash(bar_spec: &BarSpecification) -> u64 {
    let mut h = DefaultHasher::new();
    bar_spec.hash(&mut h);
    h.finish()
}

#[no_mangle]
pub extern "C" fn bar_specification_new(
    step: u64,
    aggregation: u8,
    price_type: u8,
) -> BarSpecification {
    let aggregation =
        BarAggregation::from_repr(aggregation as usize).expect("cannot parse enum value");
    let price_type = PriceType::from_repr(price_type as usize).expect("cannot parse enum value");
    BarSpecification {
        step,
        aggregation,
        price_type,
    }
}

#[no_mangle]
pub extern "C" fn bar_specification_eq(lhs: &BarSpecification, rhs: &BarSpecification) -> u8 {
    u8::from(lhs == rhs)
}

#[no_mangle]
pub extern "C" fn bar_specification_lt(lhs: &BarSpecification, rhs: &BarSpecification) -> u8 {
    u8::from(lhs < rhs)
}

#[no_mangle]
pub extern "C" fn bar_specification_le(lhs: &BarSpecification, rhs: &BarSpecification) -> u8 {
    u8::from(lhs <= rhs)
}

#[no_mangle]
pub extern "C" fn bar_specification_gt(lhs: &BarSpecification, rhs: &BarSpecification) -> u8 {
    u8::from(lhs > rhs)
}

#[no_mangle]
pub extern "C" fn bar_specification_ge(lhs: &BarSpecification, rhs: &BarSpecification) -> u8 {
    u8::from(lhs >= rhs)
}

#[no_mangle]
pub extern "C" fn bar_type_new(
    instrument_id: InstrumentId,
    spec: BarSpecification,
    aggregation_source: u8,
) -> BarType {
    let aggregation_source = AggregationSource::from_repr(aggregation_source as usize)
        .expect("Error converting enum from integer");
    BarType {
        instrument_id,
        spec,
        aggregation_source,
    }
}

#[no_mangle]
pub extern "C" fn bar_type_clone(bar_type: &BarType) -> BarType {
    bar_type.clone()
}

#[no_mangle]
pub extern "C" fn bar_type_eq(lhs: &BarType, rhs: &BarType) -> u8 {
    u8::from(lhs == rhs)
}

#[no_mangle]
pub extern "C" fn bar_type_lt(lhs: &BarType, rhs: &BarType) -> u8 {
    u8::from(lhs < rhs)
}

#[no_mangle]
pub extern "C" fn bar_type_le(lhs: &BarType, rhs: &BarType) -> u8 {
    u8::from(lhs <= rhs)
}

#[no_mangle]
pub extern "C" fn bar_type_gt(lhs: &BarType, rhs: &BarType) -> u8 {
    u8::from(lhs > rhs)
}

#[no_mangle]
pub extern "C" fn bar_type_ge(lhs: &BarType, rhs: &BarType) -> u8 {
    u8::from(lhs >= rhs)
}

#[no_mangle]
pub extern "C" fn bar_type_hash(bar_type: &BarType) -> u64 {
    let mut h = DefaultHasher::new();
    bar_type.hash(&mut h);
    h.finish()
}

/// Returns a [`BarType`] as a C string pointer.
#[no_mangle]
pub extern "C" fn bar_type_to_cstr(bar_type: &BarType) -> *const c_char {
    str_to_cstr(&bar_type.to_string())
}

#[no_mangle]
pub extern "C" fn bar_type_drop(bar_type: BarType) {
    drop(bar_type); // Memory freed here
}

/// Returns a [`Bar`] as a C string.
#[no_mangle]
pub extern "C" fn bar_to_cstr(bar: &Bar) -> *const c_char {
    str_to_cstr(&bar.to_string())
}

#[no_mangle]
pub extern "C" fn bar_clone(bar: &Bar) -> Bar {
    bar.clone()
}

#[no_mangle]
pub extern "C" fn bar_drop(bar: Bar) {
    drop(bar); // Memory freed here
}

#[no_mangle]
pub extern "C" fn bar_eq(lhs: &Bar, rhs: &Bar) -> u8 {
    u8::from(lhs == rhs)
}

#[no_mangle]
pub extern "C" fn bar_hash(bar: &Bar) -> u64 {
    let mut h = DefaultHasher::new();
    bar.hash(&mut h);
    h.finish()
}
