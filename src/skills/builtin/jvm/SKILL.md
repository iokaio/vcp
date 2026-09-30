# Java and Kotlin build conventions

Original VCP guidance.

## Read the project

- `pom.xml` and any parent POM (`<parent>`, `<modules>`): Maven build, module list, plugin and dependency management, profiles.
- `settings.gradle(.kts)` and `build.gradle(.kts)`: Gradle build, included projects, applied plugins, test task configuration. Root `pom.xml`, `build.gradle(.kts)` and `settings.gradle(.kts)` files are detected; nested-only projects are not, so the user can activate it with `/skills activate <id>` (the ID `/skills list` shows) or `vcp run --skill <id>`.
- `gradle/libs.versions.toml`: version catalog; change versions there, not inline.
- `gradlew`/`gradlew.bat` with `gradle/wrapper/gradle-wrapper.properties`, or `mvnw`/`mvnw.cmd` with `.mvn/wrapper/`: the build tool version the project expects.
- Toolchain declarations (`java { toolchain { ... } }`, `maven-toolchains-plugin`, `.java-version`, `.sdkmanrc`): the JDK the build targets.
- `src/main/java` beside `src/main/kotlin`: a mixed codebase. For Kotlin/Java interop, inspect generated/public signatures and callers rather than assuming source syntax preserves behavior.

Select the affected module and its existing test task or filter; understand whether dependent modules must also build.

## Discover commands in this order

1. Project-declared entry points: Makefile/justfile, CI workflows, CONTRIBUTING, AGENTS.md, custom Gradle tasks or Maven profiles.
2. The wrapper. Do not replace a wrapper with a global Maven/Gradle binary without validating equivalence.
3. Ecosystem defaults, as candidates only when evidence supports them:
   - `./gradlew test`, narrowed with `./gradlew :<module>:test --tests <Class>`
   - `./mvnw test`, or `./mvnw -pl <module> -am test` to include upstream modules, narrowed with `-Dtest=<Class>`
   - lint/format tasks the build already applies, such as `spotlessCheck`, `checkstyleMain`, `ktlintCheck` or `detekt`

Inspect the wrapper and repository configuration before execution: build plugins can execute arbitrary tasks, and the wrapper scripts themselves download the Gradle or Maven distribution when it is not cached. Use the wrapper consistently rather than mixing it with a global `mvn` or `gradle`. Dependency resolution downloads artifacts and plugins; when the caches are provisioned, add `--offline` (Gradle) or `-o` (Maven) so a missing artifact fails instead of downloading, and otherwise treat the first build as a network effect needing authority.

## Toolchain variants

- Maven vs Gradle; Groovy vs Kotlin DSL; convention plugins in `buildSrc/` or included builds (`build-logic/`).
- Android Gradle projects: `app/` module, variant-specific tasks such as `testDebugUnitTest`; instrumented tests need a device.
- Spring Boot, Quarkus or Micronaut plugins adding their own tasks.
- JUnit legacy (vintage) vs Jupiter APIs vs TestNG; Kotest or Spock with their own filters.
- Annotation processors and KAPT/KSP generating sources under `build/` or `target/`.

## Coding rules

Keep package/API compatibility, nullability, error and resource handling, and concurrency conventions. Do not edit generated sources; change the input and regenerate through the build.

## Verification evidence

- Run the narrowest test covering the change, then the module's test task, then broader modules only when shared code changed.
- Run the formatting, lint or static analysis the build already wires in; do not add new plugins.
- Report the JVM, wrapper version, module and tasks actually used. Missing JDKs, distribution caches, or credentials are explicit not-run constraints; do not retrieve credentials from project samples or initiate dependency downloads implicitly.
- A compile pass is not a test pass; say which one you observed.

## Pitfalls

- On Windows use `gradlew.bat` or `mvnw.cmd`; the POSIX scripts need a Unix shell and may lack execute permission or have CRLF endings that break them.
- The Gradle daemon and IDE processes hold locks on jars in `build/`; a failed `clean` on Windows is often file locking, not a build error.
- Deep module paths and generated sources can exceed Windows path length limits.
- `JAVA_HOME` may point at a different JDK than the toolchain; check before blaming code.
- Gradle's build and configuration caches can hide stale output; note when results came from cache.
- `-Dtest` filters that match nothing can fail or silently pass depending on plugin settings.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
