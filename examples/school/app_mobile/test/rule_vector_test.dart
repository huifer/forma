// 规则一致性向量 · Dart 端行为验证（P1-3 / F-004）。
//
// 与 `crates/forma/tests/rule_vectors.rs` 的向量**同源**：
//   - 同一组语义用例（边界值 / 接受 / 拒绝）；
//   - Rust（权威）侧断言三端生成物的字面量一致；
//   - 本文件在 **Flutter CI**（`flutter test`）中真实执行 Dart 校验器，
//     证明生成的 Dart 校验行为与向量一致。
//
// Rust 侧的行为验证在 e2e 冒烟（校验失败 → 400）——服务端权威（ADR-0004）。

import 'package:flutter_test/flutter_test.dart';

import 'package:school/api/api.dart';
import 'package:school/models/models.dart';
import 'package:school/validation/validators.dart';

bool _hasField(List<FieldError> errs, String field) =>
    errs.any((e) => e.field == field);

void main() {
  group('规则一致性向量 · Student.age（min 6 / max 80）', () {
    StudentInput input(int? age) => StudentInput(
          school: 's1',
          name: '学生',
          email: 'vector@example.test',
          age: age,
        );

    test('V1 低于下界（5 < 6）→ 拒绝', () {
      final errs = validateCreateStudent(input(5));
      expect(_hasField(errs, 'age'), isTrue, reason: 'age=5 必须被拒绝');
    });

    test('V2 边界值（6）→ 通过', () {
      final errs = validateCreateStudent(input(6));
      expect(_hasField(errs, 'age'), isFalse, reason: 'age=6 必须通过');
    });

    test('V3 上界（80）→ 通过', () {
      final errs = validateCreateStudent(input(80));
      expect(_hasField(errs, 'age'), isFalse, reason: 'age=80 必须通过');
    });

    test('V4 超过上界（81 > 80）→ 拒绝', () {
      final errs = validateCreateStudent(input(81));
      expect(_hasField(errs, 'age'), isTrue, reason: 'age=81 必须被拒绝');
    });

    test('V0 可空字段缺省 → 不触发范围校验', () {
      final errs = validateCreateStudent(input(null));
      expect(_hasField(errs, 'age'), isFalse, reason: 'age 缺省不应触发范围校验');
    });
  });

  group('规则一致性向量 · Course.capacity（min 1 / max 500，必填）', () {
    CourseInput input(int capacity) => CourseInput(
          school: 's1',
          title: '课程',
          capacity: capacity,
        );

    test('V5 低于下界（0 < 1）→ 拒绝', () {
      final errs = validateCreateCourse(input(0));
      expect(_hasField(errs, 'capacity'), isTrue, reason: 'capacity=0 必须被拒绝');
    });

    test('V6 边界值（1）→ 通过', () {
      final errs = validateCreateCourse(input(1));
      expect(_hasField(errs, 'capacity'), isFalse, reason: 'capacity=1 必须通过');
    });

    test('V7 上界（500）→ 通过', () {
      final errs = validateCreateCourse(input(500));
      expect(_hasField(errs, 'capacity'), isFalse, reason: 'capacity=500 必须通过');
    });

    test('V8 超过上界（501 > 500）→ 拒绝', () {
      final errs = validateCreateCourse(input(501));
      expect(_hasField(errs, 'capacity'), isTrue, reason: 'capacity=501 必须被拒绝');
    });
  });

  group('规则一致性向量 · required（三端同文案）', () {
    test('V9 空字符串 → 拒绝（name 必填）', () {
      final errs = validateCreateStudent(StudentInput(
        school: 's1',
        name: '   ',
        email: 'vector@example.test',
      ));
      expect(_hasField(errs, 'name'), isTrue, reason: '空 name 必须被拒绝');
    });
  });

  group('规则一致性向量 · check 规则（enrolledAt >= createdAt）', () {
    test('V10 报名时间早于创建时间 → 拒绝', () {
      final errs = validateCreateEnrollment(EnrollmentInput(
        student: 'st1',
        course: 'c1',
        enrolledAt: DateTime.utc(2026, 1, 1, 8, 0),
        createdAt: DateTime.utc(2026, 1, 1, 9, 0),
      ));
      expect(_hasField(errs, '__check'), isTrue, reason: 'enrolledAt < createdAt 必须被拒绝');
    });

    test('V11 报名时间不早于创建时间 → 通过', () {
      final errs = validateCreateEnrollment(EnrollmentInput(
        student: 'st1',
        course: 'c1',
        enrolledAt: DateTime.utc(2026, 1, 1, 10, 0),
        createdAt: DateTime.utc(2026, 1, 1, 9, 0),
      ));
      expect(_hasField(errs, '__check'), isFalse, reason: 'enrolledAt >= createdAt 必须通过');
    });
  });
}
