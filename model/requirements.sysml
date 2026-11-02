// Clewtrace product requirements, SysML v2 textual notation.
// This file is the single source of truth for every requirement ID cited in code, tests,
// commits, ADRs, and deps.toml (CT-DEV-001). Change it only with John's approval.

package ClewtraceRequirements {
    doc /* Product requirements for Clewtrace, a local-first SysML v2 and requirements
     * traceability tool. Core tenets: efficiency, usability, traceability.
     * When tenets conflict: traceability, then efficiency, then usability. */

    private import ScalarValues::*;

    requirement def ProductRequirement {
        doc /* A Clewtrace product requirement. verificationMethod is one of
         * Test, Analysis, Inspection, Demonstration. phase is the roadmap phase (0 to 5)
         * that delivers it; CI enforces coverage only for phases already started. */
        attribute verificationMethod : String;
        attribute phase : Integer;
    }

    package Efficiency {
        doc /* Size, speed, and memory budgets measured on the reference machine (4 GB RAM, 2 cores). */

        requirement <'CT-EFF-001'> installerSize : ProductRequirement {
            doc /* Each platform installer shall be 20 MB or smaller. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 0;
        }

        requirement <'CT-EFF-002'> idleMemory : ProductRequirement {
            doc /* The application, including its web view, shall use no more than the following total memory when idle with an empty project: 216 MB on Linux, measured as proportional set size (PSS); 81 MB on Windows, measured as private working set; and 250 MB on macOS, measured as resident memory. These budgets were set by task 2.5b from three CI runs. Task 2.10b raised the Windows budget from 74 MB to 81 MB, with John's approval on PR 48: the highest Windows total in the 13 CI runs on main and PR 48 since task 2.5b was 74.07 MB, and 81 MB is that value plus about 10%. A budget may be lowered. Raising it needs John's approval, recorded with the measurement that justifies it. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }

        requirement <'CT-EFF-003'> coldStart : ProductRequirement {
            doc /* The application shall show an interactive main window within 1.5 s of launch on the CI runner (4 GB RAM, 2 cores), measured as the median of 3 launches after one warm-up launch that is run and not counted. The cold start is held to CT-EFF-011. This budget was set by task 2.5b from three CI runs. A budget may be lowered. Raising it needs John's approval, recorded with the measurement that justifies it. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }

        requirement <'CT-EFF-004'> largeProject : ProductRequirement {
            doc /* The application shall open a project of 10,000 requirements and 50,000 trace links within 3 s and hold it in 400 MB or less. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }

        requirement <'CT-EFF-005'> uiLatency : ProductRequirement {
            doc /* Selecting, editing, filtering, and navigating shall respond within 100 ms. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }

        requirement <'CT-EFF-006'> searchLatency : ProductRequirement {
            doc /* Full-text search over 10,000 requirements shall return results within 200 ms. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }

        requirement <'CT-EFF-007'> incrementalIndex : ProductRequirement {
            doc /* The application shall re-parse and re-index only the files that changed. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 1;
        }

        requirement <'CT-EFF-008'> idleCpu : ProductRequirement {
            doc /* The application shall use no CPU while idle beyond responding to OS events (no polling loops). */
            attribute :>> verificationMethod = "Analysis";
            attribute :>> phase = 2;
        }

        requirement <'CT-EFF-009'> perfGate : ProductRequirement {
            doc /* CI shall run the performance tests in an environment limited by the operating system to 4 GB RAM and 2 CPU cores, with no swap, and fail the build when any EFF budget is exceeded. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 1;
        }

        requirement <'CT-EFF-010'> appProcessMemory : ProductRequirement {
            doc /* The application's own process, not counting its web view, shall use no more than the following memory when idle with an empty project: 93 MB on Linux, measured as PSS; 8 MB on Windows, measured as private working set; and 105 MB on macOS, measured as resident memory. These budgets were set by task 2.5b from three CI runs. A budget may be lowered. Raising it needs John's approval, recorded with the measurement that justifies it. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }

        requirement <'CT-EFF-011'> referenceStart : ProductRequirement {
            doc /* The application shall show an interactive main window within 2 s of launch on the reference machine. The Gate 2 check records the time on that machine. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 2;
        }
    }

    package Usability {
        doc /* Productive without SysML syntax, keyboard-first, accessible. */

        requirement <'CT-USE-001'> formEditing : ProductRequirement {
            doc /* Users shall create, edit, and delete requirements and their trace links through forms without writing SysML text, and each form save shall produce valid SysML v2 text. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 3;
        }

        requirement <'CT-USE-002'> commandPalette : ProductRequirement {
            doc /* Every user action shall be reachable from a keyboard-driven command palette. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 2;
        }

        requirement <'CT-USE-003'> undoRedo : ProductRequirement {
            doc /* Every edit shall be undoable and redoable within the session. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }

        requirement <'CT-USE-004'> accessibility : ProductRequirement {
            doc /* The user interface shall meet WCAG 2.1 Level AA. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 2;
        }

        requirement <'CT-USE-005'> themes : ProductRequirement {
            doc /* The application shall provide light and dark themes and follow the OS setting by default. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 2;
        }

        requirement <'CT-USE-006'> firstRun : ProductRequirement {
            doc /* A first-run sample project shall let a new user create and trace a first requirement within 5 minutes. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 3;
        }

        requirement <'CT-USE-007'> actionableErrors : ProductRequirement {
            doc /* Every error message shall state what went wrong, where (file, line, and column for model errors), and how to fix it. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 1;
        }

        requirement <'CT-USE-008'> autosave : ProductRequirement {
            doc /* The application shall save edits to disk automatically and lose no saved work after a crash or power loss. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }

        requirement <'CT-USE-009'> concurrentUse : ProductRequirement {
            doc /* The application and the CLI shall work on the same project at the same time without errors or lost work. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }
    }

    package Traceability {
        doc /* What the product does for users' requirements and trace links. */

        requirement <'CT-TRC-001'> requirementFields : ProductRequirement {
            doc /* Each requirement shall carry a unique ID, title, statement, rationale, verification method (Test, Analysis, Inspection, Demonstration), status, and owner. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 1;
        }

        requirement <'CT-TRC-002'> linkTypes : ProductRequirement {
            doc /* The application shall support derives, refines, satisfies, verifies, implements, and evidences trace links. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 1;
        }

        requirement <'CT-TRC-003'> traceMatrix : ProductRequirement {
            doc /* The application shall display a filterable, exportable trace matrix between any two sets of elements. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 3;
        }

        requirement <'CT-TRC-004'> coverageGaps : ProductRequirement {
            doc /* The application shall report requirements lacking a parent, implementation, or verification, and elements traced to no requirement. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 1;
        }

        requirement <'CT-TRC-005'> impactAnalysis : ProductRequirement {
            doc /* For a selected element, the application shall list every element reachable through trace links upstream and downstream. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 1;
        }

        requirement <'CT-TRC-006'> codeAnnotations : ProductRequirement {
            doc /* The application shall read trace annotations in source and test files and create implements and verifies links from them. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 1;
        }

        requirement <'CT-TRC-007'> baselines : ProductRequirement {
            doc /* The application shall create named baselines tied to a Git commit and compare any two baselines. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 4;
        }

        requirement <'CT-TRC-008'> headlessCli : ProductRequirement {
            doc /* A command-line interface shall run every trace check and report without the GUI and return CI-friendly exit codes. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 1;
        }

        requirement <'CT-TRC-009'> reproducibleReports : ProductRequirement {
            doc /* Every report shall record the Git commit, tool version, and a SHA-256 hash of its inputs so anyone can regenerate and verify it. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 1;
        }
    }

    package Modeling {
        doc /* Text-first modeling with a growing language subset. */

        requirement <'CT-MOD-001'> textIsTruth : ProductRequirement {
            doc /* SysML v2 textual notation files (.sysml) shall be the single source of truth for project data. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 1;
        }

        requirement <'CT-MOD-002'> initialSubset : ProductRequirement {
            doc /* The parser shall support packages, imports, comments and doc, part, attribute, port, and requirement definitions and usages, short names, satisfy, and verify. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 1;
        }

        requirement <'CT-MOD-003'> fastDiagnostics : ProductRequirement {
            doc /* The parser shall report syntax and reference errors with file, line, and column within 1 s of an edit or save in a file of up to 10,000 lines. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }

        requirement <'CT-MOD-004'> textEditor : ProductRequirement {
            doc /* The text editor shall provide SysML v2 syntax highlighting, completion, and go-to-definition. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 2;
        }

        requirement <'CT-MOD-005'> modelBrowser : ProductRequirement {
            doc /* A model browser tree shall stay synchronized with the text as the user edits. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 2;
        }

        requirement <'CT-MOD-006'> generatedDiagrams : ProductRequirement {
            doc /* The application shall generate structure, interconnection, and requirement diagram views with automatic layout. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 5;
        }

        requirement <'CT-MOD-007'> conformance : ProductRequirement {
            doc /* Parser conformance shall be measured against the example models in the OMG SysML v2 release and reported per release. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 3;
        }
    }

    package Interchange {
        doc /* Getting data in and out without lock-in. */

        requirement <'CT-XCH-001'> reqif : ProductRequirement {
            doc /* The application shall import and export requirements and trace links as ReqIF 1.2. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 3;
        }

        requirement <'CT-XCH-002'> csvXlsx : ProductRequirement {
            doc /* The application shall import and export requirements as CSV and export reports as XLSX. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 3;
        }

        requirement <'CT-XCH-003'> htmlReports : ProductRequirement {
            doc /* Every report shall be exportable as a self-contained, printable HTML file. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 3;
        }

        requirement <'CT-XCH-004'> v1Import : ProductRequirement {
            doc /* The application shall import requirements, blocks, and their relationships from SysML v1 XMI exported by Cameo. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 5;
        }

        requirement <'CT-XCH-005'> standardApi : ProductRequirement {
            doc /* The application shall provide read access through the OMG Systems Modeling API. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 5;
        }
    }

    package Compliance {
        doc /* Helping users trace NIST SP 800-171 controls to evidence. */

        requirement <'CT-CMP-001'> nist800171 : ProductRequirement {
            doc /* The application shall ship NIST SP 800-171 Rev 2 (110 requirements) and SP 800-171A (320 assessment objectives) as an importable control catalog. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 4;
        }

        requirement <'CT-CMP-002'> cmmcLevel1 : ProductRequirement {
            doc /* The application shall ship the 15 CMMC Level 1 requirements from FAR 52.204-21 as a control catalog. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 4;
        }

        requirement <'CT-CMP-003'> catalogAsData : ProductRequirement {
            doc /* Control catalogs shall be versioned data files, so a revised catalog can be loaded without a code change. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 4;
        }

        requirement <'CT-CMP-004'> controlTrace : ProductRequirement {
            doc /* Users shall trace each control to implementing elements, responsible parties, and evidence files recorded by path and SHA-256 hash. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 4;
        }

        requirement <'CT-CMP-005'> sprsScore : ProductRequirement {
            doc /* The application shall calculate a DoD Assessment Methodology (SPRS) score from control status using the published 1, 3, and 5 point weights from a maximum of 110. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 4;
        }

        requirement <'CT-CMP-006'> poam : ProductRequirement {
            doc /* The application shall track POA&M items with owner, milestone, and due date, and flag items that are not POA&M-eligible or open longer than 180 days. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 4;
        }

        requirement <'CT-CMP-007'> sspExport : ProductRequirement {
            doc /* The application shall export SSP traceability tables (control, implementation statement, responsible party, evidence) as CSV and HTML. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 4;
        }

        requirement <'CT-CMP-008'> noCertClaim : ProductRequirement {
            doc /* The application and its documentation shall not claim that using it makes an organization CMMC compliant or certified. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 0;
        }
    }

    package Security {
        doc /* Safe to run inside a CUI environment. */

        requirement <'CT-SEC-001'> noTelemetry : ProductRequirement {
            doc /* The application shall contain no telemetry and make no network connection unless the user explicitly invokes a network feature. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 0;
        }

        requirement <'CT-SEC-002'> offlineEdition : ProductRequirement {
            doc /* Any network code shall be behind a build option that is off by default, so that a build without it is an offline edition with no network code. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 3;
        }

        requirement <'CT-SEC-003'> localData : ProductRequirement {
            doc /* Project data shall stay in user-chosen local folders and shall not be copied elsewhere by the application. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 1;
        }

        requirement <'CT-SEC-004'> changeLog : ProductRequirement {
            doc /* Each project shall keep an append-only, hash-chained change log (who, when, what) whose integrity is verified on open. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 4;
        }

        requirement <'CT-SEC-005'> osEncryption : ProductRequirement {
            doc /* The application shall rely on OS full-disk encryption for data at rest and document how to enable FIPS-validated encryption on each platform. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 3;
        }

        requirement <'CT-SEC-006'> noCustomCrypto : ProductRequirement {
            doc /* The application shall implement no cryptography of its own and shall hash and verify signatures only through vetted libraries. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 0;
        }

        requirement <'CT-SEC-007'> aiConsent : ProductRequirement {
            doc /* Optional AI features shall be off by default, show exactly what will be sent, require consent per action, and support local models. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 5;
        }

        requirement <'CT-SEC-008'> safeImport : ProductRequirement {
            doc /* Importers shall be fuzz-tested and shall never execute content from imported files. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 3;
        }

        requirement <'CT-SEC-009'> robustInput : ProductRequirement {
            doc /* The application and CLI shall reject malformed, deeply nested, or oversized input files with an actionable error, and shall not crash, hang, or exceed their memory budgets. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 2;
        }
    }

    package Platform {
        doc /* Installs on mainstream desktops, runs offline, and licenses without phoning home. */

        requirement <'CT-PLT-001'> installers : ProductRequirement {
            doc /* CI shall produce installers for Windows 10 and newer (MSI) on x64, macOS 13 and newer (a universal DMG for x64 and Apple silicon), and Linux (deb, rpm) on x64 and ARM64. A Windows ARM64 installer is optional. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 0;
        }

        requirement <'CT-PLT-002'> perUserInstall : ProductRequirement {
            doc /* The application shall install per user without administrator rights where the OS allows. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 3;
        }

        requirement <'CT-PLT-003'> offline : ProductRequirement {
            doc /* The application shall run fully offline after installation. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 0;
        }

        requirement <'CT-PLT-004'> manualUpdates : ProductRequirement {
            doc /* Update checks shall be off by default and run only when the user asks. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 5;
        }

        requirement <'CT-PLT-005'> offlineLicense : ProductRequirement {
            doc /* The application shall validate a signed license file offline, with no network connection, and show the license status and any remaining trial time. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 3;
        }

        requirement <'CT-PLT-006'> trialTerms : ProductRequirement {
            doc /* The application shall allow unlimited personal, non-commercial use and 90 days of commercial evaluation without a license file, matching the license's Additional Use Grant. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 3;
        }

        requirement <'CT-PLT-007'> noDataLockout : ProductRequirement {
            doc /* When a trial or license expires, the application shall keep every project readable and exportable and disable only editing. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 3;
        }
    }

    package Development {
        doc /* How the product itself is built and proven. */

        requirement <'CT-DEV-001'> reqsInRepo : ProductRequirement {
            doc /* The product requirements shall be maintained as SysML v2 text in the repository at model/requirements.sysml. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 0;
        }

        requirement <'CT-DEV-002'> everyLineTraced : ProductRequirement {
            doc /* Every non-blank, non-comment line of first-party source code shall lie within a scope annotated with at least one valid requirement ID. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 0;
        }

        requirement <'CT-DEV-003'> everyReqCovered : ProductRequirement {
            doc /* Every requirement shall be implemented by at least one annotated code scope and verified by at least one annotated test, or carry a recorded waiver. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 1;
        }

        requirement <'CT-DEV-004'> ciGate : ProductRequirement {
            doc /* CI shall reject any change that leaves an untraced line, cites an unknown requirement ID, or lacks a requirement ID in its commit message. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 0;
        }

        requirement <'CT-DEV-005'> commitTrailer : ProductRequirement {
            doc /* Every commit message shall carry at least one Refs trailer naming a requirement ID. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 0;
        }

        requirement <'CT-DEV-006'> releaseEvidence : ProductRequirement {
            doc /* Each release shall publish a CycloneDX SBOM, build provenance attestation, SHA-256 checksums, and the full trace report. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 3;
        }

        requirement <'CT-DEV-007'> reproducibleBuilds : ProductRequirement {
            doc /* Building the same release tag twice shall produce byte-identical artifacts, with any exception documented. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 3;
        }

        requirement <'CT-DEV-008'> dependencyPolicy : ProductRequirement {
            doc /* Every third-party dependency shall be recorded with its license, justification, and the requirement it serves, and CI shall block disallowed licenses and known vulnerabilities. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 0;
        }

        requirement <'CT-DEV-009'> publicSource : ProductRequirement {
            doc /* The source code shall be publicly available for audit under the Business Source License 1.1, each release converting to Apache-2.0 four years after it ships, with public issue tracking. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 0;
        }

        requirement <'CT-DEV-010'> adrs : ProductRequirement {
            doc /* Architecture decisions shall be recorded as ADRs that cite the requirements they serve. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 0;
        }

        requirement <'CT-DEV-011'> ssdf : ProductRequirement {
            doc /* Development practices shall be mapped to NIST SP 800-218 (SSDF) in docs/SSDF.md. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 3;
        }

        requirement <'CT-DEV-012'> releasesFromCi : ProductRequirement {
            doc /* Release artifacts shall be built only by CI from a signed, tagged commit. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 0;
        }

        requirement <'CT-DEV-013'> noOutsideCode : ProductRequirement {
            doc /* The project shall accept bug reports and suggestions through public issues but no outside code contributions. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 0;
        }

        requirement <'CT-DEV-014'> licenseNotices : ProductRequirement {
            doc /* Each release shall include a third-party license notices file generated from the dependency tree. */
            attribute :>> verificationMethod = "Inspection";
            attribute :>> phase = 0;
        }
    }
}
