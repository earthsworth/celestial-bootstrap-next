use git2::{Error, Repository};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum FastForwardStatus {
    UpToDate,
    FastForward,
}

pub fn fast_forward(repo: &Repository, branch: &str) -> Result<FastForwardStatus, Error> {

    repo.find_remote("origin")?
        .fetch(&[branch], None, None)?;

    let fetch_head = repo.find_reference("FETCH_HEAD")?;
    let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;
    let analysis = repo.merge_analysis(&[&fetch_commit])?;
    if analysis.0.is_up_to_date() {
        Ok(FastForwardStatus::UpToDate)
    } else if analysis.0.is_fast_forward() {
        let refname = format!("refs/heads/{}", branch);
        let mut reference = repo.find_reference(&refname)?;
        reference.set_target(fetch_commit.id(), "Fast-Forward")?;
        repo.set_head(&refname)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        Ok(FastForwardStatus::FastForward)
    } else {
        Err(Error::from_str("Fast-forward only!"))
    }
}