//! Shared test helpers.

use std::sync::atomic::{AtomicU64, Ordering};

use uuid::Uuid;

use crate::ids::{AssignmentId, CategoryId, CourseId};
use crate::model::{Assignment, Category, Course};

static NEXT: AtomicU64 = AtomicU64::new(1);

pub fn uid() -> Uuid {
    Uuid::from_u128(u128::from(NEXT.fetch_add(1, Ordering::Relaxed)))
}

pub fn asg(possible: f64, score: Option<f64>) -> Assignment {
    let mut a = Assignment::new(AssignmentId(uid()), "a", possible);
    a.score = score;
    a
}

pub fn cat(weight: f64, drop: u32, assignments: Vec<Assignment>) -> Category {
    let mut c = Category::new(CategoryId(uid()), "c", weight);
    c.drop_lowest = drop;
    c.assignments = assignments;
    c
}

pub fn course(categories: Vec<Category>) -> Course {
    let mut c = Course::new(CourseId(uid()), "course", 3.0);
    c.categories = categories;
    c
}

pub fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
