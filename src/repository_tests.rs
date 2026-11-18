use super::{GithubRepository, normalize_github_url};

fn normalized(input: &str) -> GithubRepository {
    normalize_github_url(input).expect("input should be accepted")
}

#[test]
fn accepts_owner_repository_shorthand() {
    assert_eq!(
        normalized("cooklang/recipes"),
        GithubRepository {
            url: "https://github.com/cooklang/recipes.git".to_owned(),
            owner: "cooklang".to_owned(),
            name: "recipes".to_owned(),
        }
    );
}

#[test]
fn accepts_full_github_urls() {
    let expected = normalized("cooklang/recipes");
    assert_eq!(normalized("https://github.com/cooklang/recipes"), expected);
    assert_eq!(normalized("http://github.com/cooklang/recipes"), expected);
    assert_eq!(normalized("github.com/cooklang/recipes"), expected);
    assert_eq!(
        normalized("https://www.github.com/cooklang/recipes"),
        expected
    );
}

#[test]
fn accepts_git_suffixed_urls() {
    let expected = normalized("cooklang/recipes");
    assert_eq!(
        normalized("https://github.com/cooklang/recipes.git"),
        expected
    );
    assert_eq!(
        normalized("https://github.com/cooklang/recipes.git/"),
        expected
    );
    assert_eq!(normalized("cooklang/recipes.git"), expected);
}

#[test]
fn tolerates_surrounding_whitespace_and_trailing_slashes() {
    assert_eq!(
        normalized("  https://github.com/cooklang/recipes/  "),
        normalized("cooklang/recipes")
    );
}

#[test]
fn rejects_non_repository_input() {
    assert!(normalize_github_url("").is_err());
    assert!(normalize_github_url("cooklang").is_err());
    assert!(normalize_github_url("cooklang/recipes/extra/path").is_err());
    assert!(normalize_github_url("not a repository").is_err());
    assert!(normalize_github_url("git@github.com:cooklang/recipes.git").is_err());
}
