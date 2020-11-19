pub struct IterChunck<ITER>
where
    ITER: Sized + Iterator,
{
    iter: ITER,
    size: usize,
}

impl<ITER> IterChunck<ITER>
where
    ITER: Sized + Iterator,
{
    /// Create a new Batching iterator.
    pub fn new(iter: ITER, size: usize) -> IterChunck<ITER> {
        IterChunck { iter, size }
    }

    fn chunk_size_bound(&self, size: usize) -> usize {
        if size == 0 || size == usize::MAX {
            size
        } else {
            size / self.size + if size % self.size == 0 { 0 } else { 1 }
        }
    }
}



impl<ITER> Iterator for IterChunck<ITER>
where
    ITER: Sized + std::iter::Iterator,
{
    type Item = Vec<ITER::Item>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mut v = Vec::with_capacity(self.size);
        for i in 0..self.size {
            if let Some(e) = self.iter.next() {
                v.push(e);
            } else if i == 0 {
                return None;
            } else {
                break;
            }
        }
        Some(v)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (size, info) = self.iter.size_hint();
        (
            self.chunk_size_bound(size),
            info.map(|size| self.chunk_size_bound(size)),
        )
    }
}

pub trait IterChunckExt: Sized + Iterator {
    fn by_chunck(self, chunck_size: usize) -> IterChunck<Self> {
        IterChunck::new(self, chunck_size)
    }
}

impl<I: Iterator> IterChunckExt for I {}

#[cfg(test)]
mod test {
    use super::IterChunckExt;

    #[test]
    fn test_chunck() {
        let mut i = (1..6).by_chunck(2);
        for _ in 0..2 {
            let v = i.next();
            assert_eq!(true, v.is_some());
            let v = v.unwrap();
            assert_eq!(2, v.len());
        }
        let v = i.next();
        assert_eq!(true, v.is_some());
        let v = v.unwrap();
        assert_eq!(1, v.len());
        assert_eq!(true, i.next().is_none());
    }

    #[test]
    fn test_empty_iter() {
        let v = Vec::<usize>::default();
        let mut i = v.iter().by_chunck(2);
        assert_eq!(true, i.next().is_none());
    }

    #[test]
    fn test_size_hint() {
        let i = [1,2,3,4,5].iter().by_chunck(2);
        assert_eq!((3, Some(3)), i.size_hint());

        let i = [1].iter().by_chunck(2);
        assert_eq!((1, Some(1)), i.size_hint());

        let i = (0..).by_chunck(2);
        assert_eq!((usize::MAX, None), i.size_hint());


        let i = (0..10).filter(|x| x % 2 == 0).by_chunck(3);
        assert_eq!((0, Some(4)), i.size_hint());
    }
}
