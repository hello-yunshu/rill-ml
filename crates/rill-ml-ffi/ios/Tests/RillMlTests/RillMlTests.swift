import XCTest
@testable import RillMl

final class RillMlTests: XCTestCase {

    func testVersionAndSnapshotFormat() throws {
        let v = try RillMlInfo.version()
        XCTAssertFalse(v.isEmpty, "version must not be empty")
        XCTAssertEqual(RillMlInfo.snapshotFormatVersion, 1)
    }

    func testMeanAccumulates() throws {
        let mean = try Mean()
        try mean.update(1.0)
        try mean.update(2.0)
        try mean.update(3.0)
        XCTAssertEqual(try mean.count(), 3)
        XCTAssertEqual(try mean.value(), 2.0, accuracy: 1e-12)
    }

    func testMeanJSONRoundTrip() throws {
        let mean = try Mean()
        try mean.update(10.0)
        try mean.update(20.0)
        let json = try mean.toJSON()
        let restored = try Mean.fromJSON(json)
        XCTAssertEqual(try restored.count(), 2)
        XCTAssertEqual(try restored.value(), 15.0, accuracy: 1e-12)
    }

    func testMeanClosedHandleThrows() throws {
        let mean = try Mean()
        mean.close()
        XCTAssertThrowsError(try mean.value()) { error in
            guard case RillMlError.invalidHandle = error else {
                return XCTFail("expected invalidHandle, got \(error)")
            }
        }
    }

    func testLinearRegressionLearns() throws {
        let lr = try LinearRegression(featureCount: 1, learningRate: 0.05)
        for _ in 0..<100 {
            try lr.learn(features: [2.0], target: 10.0)
        }
        let predicted = try lr.predict([2.0])
        XCTAssertEqual(predicted, 10.0, accuracy: 0.5)
        XCTAssertEqual(try lr.samplesSeen(), 100)
        XCTAssertEqual(try lr.weights().count, 1)
    }

    func testLinearRegressionJSONRoundTrip() throws {
        let lr = try LinearRegression(featureCount: 2, learningRate: 0.01)
        try lr.learn(features: [1.0, 2.0], target: 5.0)
        let json = try lr.toJSON()
        let restored = try LinearRegression.fromJSON(json)
        XCTAssertEqual(try restored.samplesSeen(), 1)
        XCTAssertEqual(try restored.predict([1.0, 2.0]),
                       try lr.predict([1.0, 2.0]),
                       accuracy: 1e-12)
    }

    func testLinearRegressionRejectsWrongFeatureCount() throws {
        let lr = try LinearRegression(featureCount: 2, learningRate: 0.01)
        XCTAssertThrowsError(try lr.learn(features: [1.0], target: 0.0)) { error in
            guard case RillMlError.invalidArgument = error else {
                return XCTFail("expected invalidArgument, got \(error)")
            }
        }
    }
}
