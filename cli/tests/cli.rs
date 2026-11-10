//! End-to-end tests: run the `grades` binary against a throwaway database.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct Db {
    path: PathBuf,
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Db {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tally-cli-test-{}-{n}-{nanos}.db",
            std::process::id()
        ));
        Db { path }
    }

    fn run(&self, args: &[&str]) -> Out {
        let out = Command::new(env!("CARGO_BIN_EXE_grades"))
            .arg("--db")
            .arg(&self.path)
            .args(args)
            .env_remove("TALLY_DB")
            .output()
            .expect("run grades");
        Out {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// Run with `--json`, require success, return the parsed output.
    fn json(&self, args: &[&str]) -> Value {
        let mut full = args.to_vec();
        full.push("--json");
        let out = self.run(&full);
        assert_eq!(
            out.code, 0,
            "{args:?} failed: {} {}",
            out.stdout, out.stderr
        );
        serde_json::from_str(&out.stdout).expect("json output")
    }

    fn ok(&self, args: &[&str]) {
        let out = self.run(args);
        assert_eq!(
            out.code, 0,
            "{args:?} failed: {} {}",
            out.stdout, out.stderr
        );
    }

    /// Calc (4 cr): homework 40% (90, 80), exams 60% (midterm 70, final pending).
    /// Current 76, projected 55, final grade at fill x is 55 + 0.3x.
    fn calc_course(&self) {
        self.ok(&["term", "add", "Fall"]);
        self.ok(&[
            "course",
            "add",
            "Fall",
            "Calc",
            "--credits",
            "4",
            "--code",
            "M1",
            "--target",
            "90",
        ]);
        self.ok(&["category", "add", "Calc", "Homework", "--weight", "40"]);
        self.ok(&["category", "add", "Calc", "Exams", "--weight", "60"]);
        let asg = |cat: &str, name: &str, score: Option<&str>, due: &str| {
            let mut a = vec![
                "assignment",
                "add",
                "Calc",
                cat,
                name,
                "--points",
                "100",
                "--due",
                due,
            ];
            if let Some(s) = score {
                a.extend(["--score", s]);
            }
            self.ok(&a);
        };
        asg("Homework", "hw1", Some("90"), "2026-09-01");
        asg("Homework", "hw2", Some("80"), "2026-09-08");
        asg("Exams", "midterm", Some("70"), "2026-10-01");
        asg("Exams", "final", None, "2026-12-10");
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut p = self.path.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(p));
        }
    }
}

fn near(v: &Value, expected: f64) {
    let got = v.as_f64().unwrap_or(f64::NAN);
    assert!((got - expected).abs() < 1e-4, "{got} != {expected}");
}

#[test]
fn course_grade_numbers() {
    let db = Db::new();
    db.calc_course();
    let v = db.json(&["course", "grade", "Calc"]);
    assert_eq!(v["result"], "course_grade");
    near(&v["report"]["current"], 76.0);
    near(&v["report"]["projected"], 55.0);
    assert_eq!(v["report"]["current_letter"], "C");

    let text = db.run(&["course", "grade", "calc"]);
    assert_eq!(text.code, 0);
    assert!(text.stdout.contains("76.00%"), "{}", text.stdout);
}

#[test]
fn solve_secured_on_track_unreachable() {
    let db = Db::new();
    db.calc_course();
    let secured = db.json(&["solve", "Calc", "--target", "50"]);
    assert_eq!(secured["solve"]["status"]["state"], "secured");
    let on_track = db.json(&["solve", "Calc", "--target", "70"]);
    assert_eq!(on_track["solve"]["status"]["state"], "on_track");
    near(&on_track["solve"]["status"]["needed"], 50.0);
    // Stored target is 90: needs ~116.67%.
    let hopeless = db.json(&["solve", "Calc"]);
    assert_eq!(hopeless["solve"]["status"]["state"], "unreachable");
    near(&hopeless["solve"]["status"]["needed"], 350.0 / 3.0);
    // Letter target: B- is 80 -> needs 83.33%.
    let letter = db.json(&["solve", "Calc", "--target", "B-"]);
    assert_eq!(letter["solve"]["status"]["state"], "on_track");
    near(&letter["solve"]["status"]["needed"], 250.0 / 3.0);
    // Locking the only remaining assignment leaves nothing to solve: grade is 85.
    let locked = db.json(&["solve", "Calc", "--target", "80", "--lock", "final=100"]);
    assert_eq!(locked["solve"]["status"]["state"], "secured");
    let human = db.run(&["solve", "Calc", "--target", "70"]);
    assert!(human.stdout.contains("on track"), "{}", human.stdout);
}

#[test]
fn scenario_compare_leaves_real_grade_unchanged() {
    let db = Db::new();
    db.calc_course();
    db.ok(&["scenario", "create", "Ace"]);
    db.ok(&["scenario", "override", "Ace", "set-score", "final", "100"]);

    let cmp = db.json(&["scenario", "compare"]);
    let cols = cmp["columns"].as_array().expect("columns");
    assert_eq!(cols.len(), 2);
    near(&cols[0]["courses"][0]["current"], 76.0);
    near(&cols[1]["courses"][0]["current"], 85.0);

    let scen = db.json(&["course", "grade", "Calc", "--scenario", "Ace"]);
    near(&scen["report"]["current"], 85.0);
    let real = db.json(&["course", "grade", "Calc"]);
    near(&real["report"]["current"], 76.0);

    let list = db.json(&["scenario", "list"]);
    assert_eq!(
        list["scenarios"][0]["overrides"].as_array().map(Vec::len),
        Some(1)
    );
    db.ok(&["scenario", "unoverride", "Ace", "0"]);
    let human = db.run(&["scenario", "compare"]);
    assert_eq!(human.code, 0);
    assert!(human.stdout.contains("Ace"), "{}", human.stdout);
}

#[test]
fn cumulative_gpa_excludes_pass_fail() {
    let db = Db::new();
    db.calc_course();
    db.ok(&["course", "add", "Fall", "PE", "--credits", "4"]);
    db.ok(&["course", "update", "PE", "--gpa-status", "pass-fail"]);
    db.ok(&["category", "add", "PE", "Work", "--weight", "100"]);
    db.ok(&[
        "assignment",
        "add",
        "PE",
        "Work",
        "run",
        "--points",
        "10",
        "--score",
        "10",
    ]);
    let v = db.json(&["gpa", "cumulative"]);
    near(&v["report"]["gpa"], 2.0);
    near(&v["report"]["credits"], 4.0);
    let courses = v["report"]["terms"][0]["courses"]
        .as_array()
        .expect("courses");
    let pe = courses
        .iter()
        .find(|c| c["name"] == "PE")
        .expect("PE entry");
    assert_eq!(pe["included"], false);
    assert_eq!(pe["exclusion"], "pass_fail");

    let term = db.json(&["gpa", "term", "Fall"]);
    near(&term["report"]["gpa"], 2.0);
    let scale = db.json(&["gpa", "scale"]);
    assert_eq!(scale["result"], "gpa_scale");
    db.ok(&["gpa", "scale", "A=4.3"]);
    let scale = db.json(&["gpa", "scale"]);
    let a = scale["scale"]["points"]
        .as_array()
        .and_then(|p| p.iter().find(|e| e["letter"] == "A"))
        .expect("A entry");
    near(&a["points"], 4.3);
}

#[test]
fn empty_cumulative_gpa_has_no_value() {
    let db = Db::new();
    let v = db.json(&["gpa", "cumulative"]);
    assert!(v["report"]["gpa"].is_null());
    let text = db.run(&["gpa", "cumulative"]);
    assert!(text.stdout.contains("n/a"), "{}", text.stdout);
}

#[test]
fn analysis_commands_produce_json() {
    let db = Db::new();
    db.calc_course();
    let impact = db.json(&["impact", "Calc"]);
    assert_eq!(impact["result"], "impact");
    assert_eq!(impact["entries"].as_array().map(Vec::len), Some(3));
    assert!(impact["entries"][0]["delta"].is_null());
    near(&impact["entries"][2]["grade_after"], 76.0);

    let trends = db.json(&["trends", "Calc"]);
    assert_eq!(trends["result"], "trends");
    assert_eq!(trends["points"].as_array().map(Vec::len), Some(3));

    let lev = db.json(&["leverage", "Calc"]);
    assert_eq!(lev["result"], "leverage");
    let entries = lev["report"]["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["name"], "final");
    assert!(entries[0]["leverage"].as_f64().unwrap_or(0.0) > 0.0);
    let with_baseline = db.json(&["leverage", "Calc", "--baseline", "50", "--no-curve"]);
    near(&with_baseline["report"]["baseline_used"], 50.0);
    for cmd in ["impact", "trends", "leverage"] {
        let human = db.run(&[cmd, "Calc"]);
        assert_eq!(human.code, 0, "{cmd}: {}", human.stderr);
    }
}

#[test]
fn curves_cutoffs_and_updates() {
    let db = Db::new();
    db.calc_course();
    db.ok(&["course", "curve", "Calc", "--add-points", "2"]);
    near(
        &db.json(&["course", "grade", "Calc"])["report"]["current"],
        78.0,
    );
    near(
        &db.json(&["course", "grade", "Calc", "--no-curve"])["report"]["current"],
        76.0,
    );
    db.ok(&["course", "cutoffs", "Calc", "C+=75"]);
    assert_eq!(
        db.json(&["course", "grade", "Calc"])["report"]["current_letter"],
        "C+"
    );
    db.ok(&[
        "assignment",
        "update",
        "midterm",
        "--curve",
        "add:10",
        "--curve",
        "max:100",
    ]);
    db.ok(&[
        "assignment",
        "update",
        "midterm",
        "--clear-curves",
        "--score",
        "75",
    ]);
    near(
        &db.json(&["course", "grade", "Calc", "--no-curve"])["report"]["current"],
        // homework 85, exams 75 -> 0.4*85 + 0.6*75
        79.0,
    );
    let show = db.json(&["course", "show", "Calc"]);
    assert_eq!(
        show["course"]["categories"].as_array().map(Vec::len),
        Some(2)
    );
    let human = db.run(&["course", "show", "Calc"]);
    assert!(human.stdout.contains("Homework"), "{}", human.stdout);
}

#[test]
fn unknown_ref_is_an_error() {
    let db = Db::new();
    db.calc_course();
    let out = db.run(&["course", "show", "nope", "--json"]);
    assert_eq!(out.code, 1);
    let v: Value = serde_json::from_str(&out.stdout).expect("json error");
    assert!(
        v["error"].as_str().unwrap_or("").contains("unknown course"),
        "{}",
        out.stdout
    );
    let plain = db.run(&["course", "show", "nope"]);
    assert_eq!(plain.code, 1);
    assert!(plain.stdout.is_empty());
    assert!(plain.stderr.contains("unknown course"), "{}", plain.stderr);
}

#[test]
fn ambiguous_ref_lists_candidates() {
    let db = Db::new();
    db.ok(&["term", "add", "Fall"]);
    db.ok(&["term", "add", "Fall"]);
    let out = db.run(&["term", "rename", "fall", "X", "--json"]);
    assert_eq!(out.code, 1);
    let v: Value = serde_json::from_str(&out.stdout).expect("json error");
    assert!(v["error"].as_str().unwrap_or("").contains("ambiguous"));
}

#[test]
fn queries_do_not_write_but_mutations_persist() {
    let db = Db::new();
    let v = db.json(&["term", "list"]);
    assert_eq!(v["terms"].as_array().map(Vec::len), Some(0));
    db.ok(&["term", "add", "Spring"]);
    db.ok(&["term", "rename", "Spring", "Spring 2027"]);
    let v = db.json(&["term", "list"]);
    assert_eq!(v["terms"][0]["name"], "Spring 2027");
    db.ok(&["term", "rm", "spring 2027"]);
    assert_eq!(
        db.json(&["term", "list"])["terms"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn add_commands_return_created_ids_in_json() {
    let db = Db::new();
    let term = db.json(&["term", "add", "Fall"]);
    assert_eq!(term["result"], "created");
    assert_eq!(term["kind"], "term");
    let term_id = term["id"].as_str().expect("term id").to_string();
    // The returned id is directly usable as a ref to chain calls.
    let course = db.json(&["course", "add", &term_id, "Calc", "--credits", "3"]);
    assert_eq!(course["kind"], "course");
    let course_id = course["id"].as_str().expect("course id").to_string();
    let cat = db.json(&["category", "add", &course_id, "HW", "--weight", "100"]);
    assert_eq!(cat["kind"], "category");
    let cat_id = cat["id"].as_str().expect("category id").to_string();
    let asg = db.json(&[
        "assignment",
        "add",
        &course_id,
        &cat_id,
        "hw1",
        "--points",
        "10",
        "--score",
        "9",
    ]);
    assert_eq!(asg["kind"], "assignment");
    let asg_id = asg["id"].as_str().expect("assignment id").to_string();
    db.ok(&["assignment", "update", &asg_id, "--score", "10"]);
    near(
        &db.json(&["course", "grade", &course_id])["report"]["current"],
        100.0,
    );
    let scen = db.json(&["scenario", "create", "S"]);
    assert_eq!(scen["kind"], "scenario");

    let human = db.run(&["term", "add", "Spring"]);
    assert_eq!(human.code, 0);
    assert!(human.stdout.starts_with("added term ["), "{}", human.stdout);
}

#[test]
fn partial_curve_cutoff_and_scale_edits() {
    let db = Db::new();
    db.calc_course();
    db.ok(&["course", "curve", "Calc", "--add-points", "2"]);
    db.ok(&["course", "curve", "Calc", "--cutoff-shift", "1"]);
    let show = db.json(&["course", "show", "Calc"]);
    near(&show["course"]["curve"]["add_points"], 2.0);
    near(&show["course"]["curve"]["cutoff_shift"], 1.0);
    db.ok(&["course", "curve", "Calc", "--clear"]);
    let show = db.json(&["course", "show", "Calc"]);
    near(&show["course"]["curve"]["add_points"], 0.0);
    near(&show["course"]["curve"]["cutoff_shift"], 0.0);

    db.ok(&["course", "cutoffs", "Calc", "A+=97", "B=84"]);
    let find = |v: &Value, l: &str| -> Option<f64> {
        v["course"]["cutoffs"]["entries"]
            .as_array()?
            .iter()
            .find(|e| e["letter"] == l)
            .and_then(|e| e["min"].as_f64())
    };
    let show = db.json(&["course", "show", "Calc"]);
    assert_eq!(find(&show, "A+"), Some(97.0));
    assert_eq!(find(&show, "B"), Some(84.0));
    assert_eq!(find(&show, "C"), Some(73.0));
    db.ok(&["course", "cutoffs", "Calc", "A+=none"]);
    assert_eq!(find(&db.json(&["course", "show", "Calc"]), "A+"), None);

    db.ok(&["gpa", "scale", "A=4.3", "B=3.1"]);
    let scale = db.json(&["gpa", "scale"]);
    let pts = |l: &str| {
        scale["scale"]["points"]
            .as_array()
            .and_then(|p| p.iter().find(|e| e["letter"] == l))
            .and_then(|e| e["points"].as_f64())
    };
    assert_eq!(pts("A"), Some(4.3));
    assert_eq!(pts("B"), Some(3.1));
    assert_eq!(pts("C"), Some(2.0));
}

#[test]
fn later_override_can_target_a_hypothetical_assignment() {
    let db = Db::new();
    db.calc_course();
    db.ok(&["scenario", "create", "Extra"]);
    db.ok(&[
        "scenario",
        "override",
        "Extra",
        "add-assignment",
        "Calc",
        "Exams",
        "quiz",
        "--points",
        "100",
        "--score",
        "0",
    ]);
    // Without the hypothetical, this name is unknown to real data.
    let out = db.run(&["assignment", "rm", "quiz", "--json"]);
    assert_eq!(out.code, 1);
    db.ok(&["scenario", "override", "Extra", "set-score", "quiz", "100"]);
    let list = db.json(&["scenario", "list"]);
    assert_eq!(
        list["scenarios"][0]["overrides"].as_array().map(Vec::len),
        Some(2)
    );
    // Real data untouched.
    near(
        &db.json(&["course", "grade", "Calc"])["report"]["current"],
        76.0,
    );
    // Solver locks can target it within the scenario too.
    let solved = db.json(&[
        "solve",
        "Calc",
        "--scenario",
        "Extra",
        "--target",
        "50",
        "--lock",
        "quiz=100",
    ]);
    assert_eq!(solved["result"], "solve");
}

#[test]
fn percent_and_letter_targets_parse_in_core() {
    let db = Db::new();
    db.calc_course();
    for t in ["70", "70.0", "70%", "c-", "C-"] {
        let v = db.json(&["solve", "Calc", "--target", t]);
        assert_eq!(v["solve"]["status"]["state"], "on_track", "{t}");
    }
    let out = db.run(&["solve", "Calc", "--target", "bogus", "--json"]);
    assert_eq!(out.code, 1);
    assert!(out.stdout.contains("invalid target"), "{}", out.stdout);
}

#[test]
fn json_mode_reports_usage_errors_as_json() {
    let db = Db::new();
    let out = db.run(&["--json", "no-such-command"]);
    assert_eq!(out.code, 2);
    let v: Value = serde_json::from_str(&out.stdout).expect("json error");
    assert!(v["error"].as_str().is_some_and(|e| !e.is_empty()));
    // Without --json: normal clap behaviour on stderr.
    let plain = db.run(&["no-such-command"]);
    assert_eq!(plain.code, 2);
    assert!(plain.stdout.is_empty());
    assert!(!plain.stderr.is_empty());
    // --help and --version still work, even with --json.
    let help = db.run(&["--json", "--help"]);
    assert_eq!(help.code, 0);
    assert!(help.stdout.contains("Usage"), "{}", help.stdout);
    let version = db.run(&["--version", "--json"]);
    assert_eq!(version.code, 0);
    assert!(version.stdout.contains("grades"), "{}", version.stdout);
}

#[test]
fn explicit_db_path_does_not_create_directories() {
    let db = Db::new();
    let missing = db.path.with_extension("").join("nested").join("tally.db");
    let out = Command::new(env!("CARGO_BIN_EXE_grades"))
        .arg("--db")
        .arg(&missing)
        .args(["term", "list"])
        .env_remove("TALLY_DB")
        .output()
        .expect("run grades");
    assert_eq!(out.status.code(), Some(1));
    assert!(!missing.parent().is_some_and(|p| p.exists()));
}

#[test]
fn resolve_maps_to_core_command() {
    let db = Db::new();
    db.calc_course();
    let t = db.json(&["resolve", "term", "fall"]);
    assert_eq!(t["result"], "resolved");
    assert_eq!(t["kind"], "term");
    assert_eq!(t["name"], "Fall");
    let c = db.json(&["resolve", "course", "M1"]);
    assert_eq!(c["name"], "Calc");
    let k = db.json(&["resolve", "category", "Exams", "--course", "Calc"]);
    assert_eq!(k["kind"], "category");
    assert_eq!(k["name"], "Exams");
    // Category without a course is an error.
    let out = db.run(&["resolve", "category", "Exams", "--json"]);
    assert_ne!(out.code, 0);
}

#[test]
fn export_then_import_round_trips_into_another_db() {
    let src = Db::new();
    src.calc_course();
    let exported = src.json(&["export"]);
    assert_eq!(exported["result"], "gradebook");
    let file = std::env::temp_dir().join(format!("tally-export-{}.json", std::process::id()));
    std::fs::write(&file, exported["gradebook"].to_string()).expect("write export");

    let dst = Db::new();
    let imported = dst.json(&["import", file.to_str().expect("utf8 path")]);
    let _ = std::fs::remove_file(&file);
    assert_eq!(imported["result"], "done");
    assert_eq!(dst.json(&["export"]), exported);

    let missing = dst.run(&["import", "definitely-missing.json"]);
    assert_ne!(missing.code, 0);
}
